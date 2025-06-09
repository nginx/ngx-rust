//! Synchronization primitives over shared memory.
//!
//! This module provides an alternative implementation for the `ngx_atomic_t` type,
//! `ngx_atomic_*`/`ngx_rwlock_*` family of functions and related usage patterns from nginx.
//!
//! `<ngx_atomic.h>` contains a wide variety of implementation variants for different platforms and
//! build configurations. It's not feasible to properly expose all of these to the Rust code, and we
//! are not going to. The implementation here uses similar logic on the foundation of the
//! [core::sync::atomic] types and is intentionally _not interoperable_ with the nginx atomics.
//! Thus, it's only suitable for use for new shared memory structures instead of, for example,
//! interacting with the upstream zones.
//!
//! One potential pitfall here is that atomics in Rust are specified in terms of threads, and we use
//! the types in this module for interprocess synchronization. This should not be an issue though,
//! as Rust refers to the C/C++11 memory model for atomics, and there's a following note in
//! [atomics.lockfree]:
//!
//! > [Note: Operations that are lock-free should also be address-free. That is, atomic operations
//! > on the same memory location via two different addresses will communicate atomically. The
//! > implementation should not depend on any per-process state. This restriction enables
//! > communication via memory that is mapped into a process more than once and by memory that is
//! > shared between two processes. — end note]
//!
//! In practice, this recommendation is applied in all the implementations that matter to us.
use core::sync::atomic::{self, Ordering};

use nginx_sys::ngx_sched_yield;

const NGX_RWLOCK_SPIN: usize = 2048;
const NGX_RWLOCK_WLOCK: usize = usize::MAX;

type NgxAtomic = atomic::AtomicUsize;

/// Raw lock type.
///
pub struct RawSpinlock(NgxAtomic);

/// Reader-writer lock over an atomic variable, based on the nginx rwlock implementation.
pub type RwLock<T> = lock_api::RwLock<RawSpinlock, T>;

/// RAII structure used to release the shared read access of a lock when dropped.
pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawSpinlock, T>;

/// RAII structure used to release the exclusive write access of a lock when dropped.
pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawSpinlock, T>;

unsafe impl lock_api::RawRwLock for RawSpinlock {
    // Only used for initialization, will not be mutated
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: RawSpinlock = RawSpinlock(NgxAtomic::new(0));

    type GuardMarker = lock_api::GuardNoSend;

    fn lock_shared(&self) {
        loop {
            if self.try_lock_shared() {
                return;
            }

            if unsafe { nginx_sys::ngx_ncpu > 1 } {
                for n in 0..NGX_RWLOCK_SPIN {
                    for _ in 0..n {
                        core::hint::spin_loop()
                    }

                    if self.try_lock_shared() {
                        return;
                    }
                }
            }

            ngx_sched_yield()
        }
    }

    fn try_lock_shared(&self) -> bool {
        let value = self.0.load(Ordering::Acquire);

        if value == NGX_RWLOCK_WLOCK {
            return false;
        }

        self.0
            .compare_exchange(value, value + 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    unsafe fn unlock_shared(&self) {
        self.0.fetch_sub(1, Ordering::Release);
    }

    fn lock_exclusive(&self) {
        loop {
            if self.try_lock_exclusive() {
                return;
            }

            if unsafe { nginx_sys::ngx_ncpu > 1 } {
                for n in 0..NGX_RWLOCK_SPIN {
                    for _ in 0..n {
                        core::hint::spin_loop()
                    }

                    if self.try_lock_exclusive() {
                        return;
                    }
                }
            }

            ngx_sched_yield()
        }
    }

    fn try_lock_exclusive(&self) -> bool {
        self.0
            .compare_exchange(0, NGX_RWLOCK_WLOCK, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    unsafe fn unlock_exclusive(&self) {
        self.0.store(0, Ordering::Release)
    }
}
