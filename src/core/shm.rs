//! Safe wrappers for nginx shared memory zones.
//!
//! Shared memory zones are declared during configuration parsing and initialized once the
//! configuration is applied.  The raw interface requires an `unsafe extern "C"` callback and a
//! hand-rolled cast of the untyped `ngx_slab_pool_t::data` pointer at every access; the types
//! here keep the zone's payload type in the signature instead.
//!
//! See <https://nginx.org/en/docs/dev/development_guide.html#shared_memory>.
use core::ffi::c_void;
use core::fmt;
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

use nginx_sys::{
    ngx_conf_t, ngx_int_t, ngx_module_t, ngx_shared_memory_add, ngx_shm_zone_t, ngx_str_t,
};

use crate::allocator::{AllocError, allocate};
use crate::core::{NgxStr, SlabPool, Status};

/// A value stored in the slab pool of a shared memory zone.
///
/// The value is created once, on the first initialization of the zone, and then reused for as
/// long as the zone keeps the same mapping — notably across configuration reloads and across a
/// binary upgrade.  It is never dropped: shared memory outlives the process that created it, so
/// implementers should not rely on [`Drop`] for cleanup.
pub trait SharedZoneData: Sized {
    /// Creates the initial value in the zone's slab pool.
    ///
    /// Everything reachable from the returned value must itself be allocated from `alloc`, or it
    /// will not be visible to the other worker processes.
    fn new_in(alloc: SlabPool) -> Result<Self, AllocError>;

    /// Accepts or rejects a value left in the zone by a previous configuration.
    ///
    /// Called instead of [`SharedZoneData::new_in`] when the zone already holds a value, which
    /// happens when a configuration reload reuses the mapping or when a zone is inherited from
    /// the master process.  Returning an error aborts the reload, leaving the running
    /// configuration in place.
    ///
    /// The default implementation accepts the existing value unchanged.
    fn reuse(&self) -> Result<(), Status> {
        Ok(())
    }
}

/// Error returned when a shared memory zone cannot be declared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ShmError {
    /// nginx refused the zone: the name is already declared for a different module, or with a
    /// conflicting size.  A message describing which of the two it was has already been written
    /// to the configuration log.
    Rejected,
    /// Allocation from the configuration pool failed.
    Alloc,
}

impl fmt::Display for ShmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected => f.write_str("shared memory zone rejected"),
            Self::Alloc => f.write_str("configuration pool allocation failed"),
        }
    }
}

impl core::error::Error for ShmError {}

impl From<ShmError> for Status {
    fn from(_: ShmError) -> Self {
        Status::NGX_ERROR
    }
}

/// A shared memory zone holding a value of type `T`.
///
/// Obtained from [`SharedZone::add`] while parsing a configuration directive, and normally kept
/// in the module's configuration.  The zone is not usable until nginx has applied the
/// configuration, so [`SharedZone::get`] returns [`None`] when called before that point.
pub struct SharedZone<T> {
    zone: NonNull<ngx_shm_zone_t>,
    _data: PhantomData<fn() -> T>,
}

impl<T> Clone for SharedZone<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SharedZone<T> {}

impl<T> fmt::Debug for SharedZone<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedZone")
            .field("name", &self.name())
            .field("size", &self.size())
            .finish()
    }
}

impl<T: SharedZoneData> SharedZone<T> {
    /// Declares a shared memory zone, or joins one already declared by this module.
    ///
    /// `name` is copied into the configuration pool, so the caller's buffer does not have to
    /// outlive the call.  `size` may be zero to join a zone whose size is declared elsewhere.
    /// `module` is used as the zone's tag: two modules may declare zones of the same name
    /// without colliding, while a second declaration from the same module must agree on the
    /// size.
    ///
    /// The zone's initialization callback is installed here; it calls
    /// [`SharedZoneData::new_in`] or [`SharedZoneData::reuse`] as appropriate.
    pub fn add(
        cf: &mut ngx_conf_t,
        name: &NgxStr,
        size: usize,
        module: &'static ngx_module_t,
    ) -> Result<Self, ShmError> {
        // SAFETY: `cf.pool` is a valid pool for the duration of configuration parsing.
        let mut name =
            unsafe { ngx_str_t::from_bytes(cf.pool, name.as_bytes()) }.ok_or(ShmError::Alloc)?;

        // SAFETY: `cf` and the freshly allocated `name` are both valid; the name's contents are
        // owned by the configuration pool and outlive the cycle.
        let zone = unsafe {
            ngx_shared_memory_add(
                cf,
                &raw mut name,
                size,
                ptr::from_ref(module).cast_mut().cast::<c_void>(),
            )
        };

        let mut zone = NonNull::new(zone).ok_or(ShmError::Rejected)?;

        // SAFETY: `ngx_shared_memory_add` returned a valid, uniquely borrowed zone.
        unsafe { zone.as_mut() }.init = Some(init_zone::<T>);

        Ok(Self { zone, _data: PhantomData })
    }

    /// Returns the value stored in the zone, or [`None`] if the zone is not initialized yet.
    ///
    /// # Panics
    ///
    /// Does not panic, but the returned reference must not be held across a configuration
    /// reload: zone addresses of an old cycle may be unmapped once the reload completes.  In
    /// practice this means the reference is valid for the whole lifetime of a worker process,
    /// and inside a cycle pool cleanup handler it is not.
    pub fn get(&self) -> Option<&T> {
        // SAFETY: the zone is allocated from the cycle pool and outlives this handle.
        let alloc = unsafe { SlabPool::from_shm_zone(self.zone.as_ref()) }?;
        // SAFETY: `data` is only ever set by `init_zone::<T>`, which stores a `T`.
        unsafe { alloc.as_ref().data.cast::<T>().as_ref() }
    }

    /// Returns the zone's slab pool, for allocating values that the payload will refer to.
    ///
    /// Returns [`None`] before the zone is mapped.
    pub fn slab_pool(&self) -> Option<SlabPool> {
        // SAFETY: the zone is allocated from the cycle pool and outlives this handle.
        unsafe { SlabPool::from_shm_zone(self.zone.as_ref()) }
    }
}

impl<T> SharedZone<T> {
    /// Returns the zone's name.
    pub fn name(&self) -> &NgxStr {
        // SAFETY: the zone and its name are allocated from the cycle pool.
        unsafe { NgxStr::from_ngx_str(self.zone.as_ref().shm.name) }
    }

    /// Returns the configured size of the zone in bytes.
    pub fn size(&self) -> usize {
        // SAFETY: the zone is allocated from the cycle pool and outlives this handle.
        unsafe { self.zone.as_ref() }.shm.size
    }

    /// Returns a pointer to the underlying zone, for interoperation with the raw API.
    pub fn as_ptr(&self) -> *mut ngx_shm_zone_t {
        self.zone.as_ptr()
    }
}

/// Initialization callback installed by [`SharedZone::add`].
///
/// The `data` argument carries the previous cycle's `ngx_shm_zone_t::data`, which this wrapper
/// never sets; the existing value is recovered from the slab pool instead, so that a zone
/// inherited from the master process is handled the same way as one reused across a reload.
unsafe extern "C" fn init_zone<T: SharedZoneData>(
    zone: *mut ngx_shm_zone_t,
    _data: *mut c_void,
) -> ngx_int_t {
    // SAFETY: nginx passes a mapped, uniquely borrowed zone to the init callback.
    let zone = unsafe { &*zone };

    match init_data::<T>(zone) {
        Ok(()) => Status::NGX_OK.into(),
        Err(err) => err.into(),
    }
}

fn init_data<T: SharedZoneData>(zone: &ngx_shm_zone_t) -> Result<(), Status> {
    // SAFETY: called from the init callback, where the zone is mapped and its slab pool has
    // already been set up by `ngx_init_zone_pool`.
    let mut alloc = unsafe { SlabPool::from_shm_zone(zone) }.ok_or(Status::NGX_ERROR)?;

    let existing = alloc.as_ref().data;
    // SAFETY: `data` is only ever set below, to a `T` allocated from this pool.
    if let Some(existing) = unsafe { existing.cast::<T>().as_ref() } {
        return existing.reuse();
    }

    let value = T::new_in(alloc.clone()).map_err(|_| Status::NGX_ERROR)?;
    let value = allocate(value, &alloc).map_err(|_| Status::NGX_ERROR)?;
    alloc.as_mut().data = value.as_ptr().cast();

    Ok(())
}
