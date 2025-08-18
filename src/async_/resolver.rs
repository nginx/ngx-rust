// Copyright (c) F5, Inc.
//
// This source code is licensed under the Apache License, Version 2.0 license found in the
// LICENSE file in the root directory of this source tree.

//! Wrapper for the nginx resolver.
//!
//! See <https://nginx.org/en/docs/http/ngx_http_core_module.html#resolver>.

use alloc::string::{String, ToString};
use core::ffi::c_void;
use core::fmt;
use core::num::NonZero;
use core::pin::Pin;
use core::ptr::NonNull;
use core::task::{Context, Poll, Waker};

use crate::{
    allocator::Box,
    collections::Vec,
    core::Pool,
    ffi::{
        ngx_addr_t, ngx_msec_t, ngx_resolve_name, ngx_resolve_start, ngx_resolver_ctx_t,
        ngx_resolver_t, ngx_str_t,
    },
};
use nginx_sys::{
    NGX_RESOLVE_FORMERR, NGX_RESOLVE_NOTIMP, NGX_RESOLVE_NXDOMAIN, NGX_RESOLVE_REFUSED,
    NGX_RESOLVE_SERVFAIL, NGX_RESOLVE_TIMEDOUT,
};

/// Error type for all uses of `Resolver`.
#[derive(Debug)]
pub enum Error {
    /// No resolver configured
    NoResolver,
    /// Resolver error, with context of name being resolved
    Resolver(ResolverError, String),
    /// Allocation failed
    AllocationFailed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::NoResolver => write!(f, "No resolver configured"),
            Error::Resolver(err, context) => write!(f, "{err}: resolving `{context}`"),
            Error::AllocationFailed => write!(f, "Allocation failed"),
        }
    }
}
impl core::error::Error for Error {}

/// These cases directly reflect the NGX_RESOLVE_ error codes,
/// plus a timeout, and a case for an unknown error where a known
/// NGX_RESOLVE_ should be.
#[derive(Debug)]
pub enum ResolverError {
    /// Format error (NGX_RESOLVE_FORMERR)
    FormErr,
    /// Server failure (NGX_RESOLVE_SERVFAIL)
    ServFail,
    /// Host not found (NGX_RESOLVE_NXDOMAIN)
    NXDomain,
    /// Unimplemented (NGX_RESOLVE_NOTIMP)
    NotImp,
    /// Operation refused (NGX_RESOLVE_REFUSED)
    Refused,
    /// Timed out (NGX_RESOLVE_TIMEDOUT)
    TimedOut,
    /// Unknown NGX_RESOLVE error
    Unknown(isize),
}
impl fmt::Display for ResolverError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ResolverError::FormErr => write!(f, "Format error"),
            ResolverError::ServFail => write!(f, "Server Failure"),
            ResolverError::NXDomain => write!(f, "Host not found"),
            ResolverError::NotImp => write!(f, "Unimplemented"),
            ResolverError::Refused => write!(f, "Refused"),
            ResolverError::TimedOut => write!(f, "Timed out"),
            ResolverError::Unknown(code) => write!(f, "Unknown NGX_RESOLVE error {code}"),
        }
    }
}
impl core::error::Error for ResolverError {}

/// Convert from the NGX_RESOLVE_ error codes.
impl From<NonZero<isize>> for ResolverError {
    fn from(code: NonZero<isize>) -> ResolverError {
        match code.get() as u32 {
            NGX_RESOLVE_FORMERR => ResolverError::FormErr,
            NGX_RESOLVE_SERVFAIL => ResolverError::ServFail,
            NGX_RESOLVE_NXDOMAIN => ResolverError::NXDomain,
            NGX_RESOLVE_NOTIMP => ResolverError::NotImp,
            NGX_RESOLVE_REFUSED => ResolverError::Refused,
            NGX_RESOLVE_TIMEDOUT => ResolverError::TimedOut,
            _ => ResolverError::Unknown(code.get()),
        }
    }
}

type Res = Result<Vec<ngx_addr_t, Pool>, Error>;

/// A wrapper for an ngx_resolver_t which provides an async Rust API
pub struct Resolver {
    resolver: NonNull<ngx_resolver_t>,
    timeout: ngx_msec_t,
}

impl Resolver {
    /// Create a new `Resolver` from existing pointer to `ngx_resolver_t` and
    /// timeout.
    pub fn from_resolver(resolver: NonNull<ngx_resolver_t>, timeout: ngx_msec_t) -> Self {
        Self { resolver, timeout }
    }

    /// Resolve a name into a set of addresses.
    pub async fn resolve_name(&self, name: &ngx_str_t, pool: &Pool) -> Res {
        let mut resolver = Resolution::new(name, &ngx_str_t::empty(), self, pool)?;
        resolver.as_mut().await
    }

    /// Resolve a service into a set of addresses.
    pub async fn resolve_service(&self, name: &ngx_str_t, service: &ngx_str_t, pool: &Pool) -> Res {
        let mut resolver = Resolution::new(name, service, self, pool)?;
        resolver.as_mut().await
    }
}

struct Resolution<'a> {
    // Storage for the result of the resolution `Res`. Populated by the
    // callback handler, and taken by the Future::poll impl.
    complete: Option<Res>,
    // Storage for a pending Waker. Populated by the Future::poll impl,
    // and taken by the callback handler.
    waker: Option<Waker>,
    // Pool used for allocating `Vec<ngx_addr_t>` contents in `Res`. Read by
    // the callback handler.
    pool: &'a Pool,
    // Owned pointer to the ngx_resolver_ctx_t.
    ctx: Option<ResolverCtx>,
}

impl<'a> Resolution<'a> {
    pub fn new(
        name: &ngx_str_t,
        service: &ngx_str_t,
        resolver: &Resolver,
        pool: &'a Pool,
    ) -> Result<Pin<Box<Self, Pool>>, Error> {
        // Create a pinned Resolution on the Pool, so that we can make
        // a stable pointer to the Resolution struct.
        let mut this = Box::pin_in(
            Resolution {
                complete: None,
                waker: None,
                pool,
                ctx: None,
            },
            pool.clone(),
        );

        // Set up the ctx with everything the resolver needs to resolve a
        // name, and the handler callback which is called on completion.
        let mut ctx = ResolverCtx::new(resolver.resolver)?;
        ctx.name = *name;
        ctx.service = *service;
        ctx.timeout = resolver.timeout;
        ctx.set_cancelable(1);
        ctx.handler = Some(Self::handler);

        {
            // Safety: Self::handler, Future::poll, and Drop::drop will have
            // access to &mut Resolution. Nginx is single-threaded and we are
            // assured only one of those is on the stack at a time, except if
            // Self::handler wakes a task which polls or drops the Future,
            // which it only does after use of &mut Resolution is complete.
            let ptr: &mut Resolution = unsafe { Pin::into_inner_unchecked(this.as_mut()) };
            ctx.data = ptr as *mut Resolution as *mut c_void;
        }

        // Neither ownership nor borrows are tracked for this pointer,
        // and ctxp will not be used after the ctx destruction.
        let ctxp = ctx.0;
        this.ctx = Some(ctx);

        // Start name resolution using the ctx. If the name is in the dns
        // cache, the handler may get called from this stack. Otherwise, it
        // will be called later by nginx when it gets a dns response or a
        // timeout.
        let ret = unsafe { ngx_resolve_name(ctxp.as_ptr()) };
        if let Some(e) = NonZero::new(ret) {
            return Err(Error::Resolver(ResolverError::from(e), name.to_string()));
        }

        Ok(this)
    }

    // Nginx will call this handler when name resolution completes. If the
    // result is in the cache, this could be called from inside ngx_resolve_name.
    // Otherwise, it will be called later on the event loop.
    unsafe extern "C" fn handler(ctx: *mut ngx_resolver_ctx_t) {
        let mut data = unsafe { NonNull::new_unchecked((*ctx).data as *mut Resolution) };
        let this: &mut Resolution = unsafe { data.as_mut() };

        if let Some(ctx) = this.ctx.take() {
            this.complete = Some(ctx.into_result(this.pool));
        }

        // Wake last, after all use of &mut Resolution, because wake may
        // poll Resolution future on current stack.
        if let Some(waker) = this.waker.take() {
            waker.wake();
        }
    }
}

impl<'a> core::future::Future for Resolution<'a> {
    type Output = Result<Vec<ngx_addr_t, Pool>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Resolution is Unpin, so we can use it as just a &mut Resolution
        let this: &mut Resolution = self.get_mut();

        // The handler populates this.complete, and we consume it here:
        match this.complete.take() {
            Some(res) => Poll::Ready(res),
            None => {
                // If the handler has not yet fired, populate the waker field,
                // which the handler will consume:
                match &mut this.waker {
                    None => {
                        this.waker = Some(cx.waker().clone());
                    }
                    Some(w) => w.clone_from(cx.waker()),
                }
                Poll::Pending
            }
        }
    }
}

/// An owned ngx_resolver_ctx_t.
struct ResolverCtx(NonNull<ngx_resolver_ctx_t>);

impl core::ops::Deref for ResolverCtx {
    type Target = ngx_resolver_ctx_t;

    fn deref(&self) -> &Self::Target {
        // SAFETY: this wrapper is always constructed with a valid non-empty resolve context
        unsafe { self.0.as_ref() }
    }
}

impl core::ops::DerefMut for ResolverCtx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: this wrapper is always constructed with a valid non-empty resolve context
        unsafe { self.0.as_mut() }
    }
}

impl Drop for ResolverCtx {
    fn drop(&mut self) {
        unsafe {
            nginx_sys::ngx_resolve_name_done(self.0.as_mut());
        }
    }
}

impl ResolverCtx {
    /// Creates a new resolver context.
    ///
    /// This implementation currently passes a null for the second argument `temp`. A non-null
    /// `temp` provides a fast, non-callback-based path for immediately returning an addr if
    /// `temp` contains a name which is textual form of an addr.
    pub fn new(resolver: NonNull<ngx_resolver_t>) -> Result<Self, Error> {
        let ctx = unsafe { ngx_resolve_start(resolver.as_ptr(), core::ptr::null_mut()) };
        NonNull::new(ctx).map(Self).ok_or(Error::AllocationFailed)
    }

    /// Take the results in a ctx and make an owned copy as a
    /// Result<Vec<ngx_addr_t, Pool>, Error>, where the Vec and the internals
    /// of the ngx_addr_t are allocated on the given Pool
    pub fn into_result(self, pool: &Pool) -> Result<Vec<ngx_addr_t, Pool>, Error> {
        if let Some(e) = NonZero::new(self.state) {
            return Err(Error::Resolver(
                ResolverError::from(e),
                self.name.to_string(),
            ));
        }
        if self.addrs.is_null() {
            Err(Error::AllocationFailed)?;
        }

        let mut out = Vec::new_in(pool.clone());

        if self.naddrs > 0 {
            out.try_reserve_exact(self.naddrs)
                .map_err(|_| Error::AllocationFailed)?;

            for addr in unsafe { core::slice::from_raw_parts(self.addrs, self.naddrs) } {
                out.push(copy_resolved_addr(addr, pool)?);
            }
        }

        Ok(out)
    }
}

/// Take the contents of an ngx_resolver_addr_t and make an owned copy as
/// an ngx_addr_t, using the Pool for allocation of the internals.
fn copy_resolved_addr(
    addr: &nginx_sys::ngx_resolver_addr_t,
    pool: &Pool,
) -> Result<ngx_addr_t, Error> {
    let sockaddr = pool.alloc(addr.socklen as usize) as *mut nginx_sys::sockaddr;
    if sockaddr.is_null() {
        Err(Error::AllocationFailed)?;
    }
    unsafe {
        addr.sockaddr
            .cast::<u8>()
            .copy_to_nonoverlapping(sockaddr.cast(), addr.socklen as usize)
    };

    let name = unsafe { ngx_str_t::from_bytes(pool.as_ptr(), addr.name.as_bytes()) }
        .ok_or(Error::AllocationFailed)?;

    Ok(ngx_addr_t {
        sockaddr,
        socklen: addr.socklen,
        name,
    })
}
