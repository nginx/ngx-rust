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
    /// Unexpected error
    Unexpected(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::NoResolver => write!(f, "No resolver configured"),
            Error::Resolver(err, context) => write!(f, "{err}: resolving `{context}`"),
            Error::AllocationFailed => write!(f, "Allocation failed"),
            Error::Unexpected(err) => write!(f, "Unexpected error: {err}"),
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
    /// Operatio refused (NGX_RESOLVE_REFUSED)
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

/// Convert from the NGX_RESOLVE_ error codes. Fails if code was success.
impl TryFrom<isize> for ResolverError {
    type Error = ();
    fn try_from(code: isize) -> Result<ResolverError, Self::Error> {
        match code as u32 {
            0 => Err(()),
            NGX_RESOLVE_FORMERR => Ok(ResolverError::FormErr),
            NGX_RESOLVE_SERVFAIL => Ok(ResolverError::ServFail),
            NGX_RESOLVE_NXDOMAIN => Ok(ResolverError::NXDomain),
            NGX_RESOLVE_NOTIMP => Ok(ResolverError::NotImp),
            NGX_RESOLVE_REFUSED => Ok(ResolverError::Refused),
            NGX_RESOLVE_TIMEDOUT => Ok(ResolverError::TimedOut),
            _ => Ok(ResolverError::Unknown(code)),
        }
    }
}

type Res = Result<Vec<ngx_addr_t>, Error>;

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
    ///
    /// The set of addresses may not be deterministic, because the
    /// implementation of the resolver may race multiple DNS requests.
    pub async fn resolve(&self, name: &ngx_str_t, pool: &Pool) -> Res {
        let mut resolver = Resolution::new(name, pool, self.resolver, self.timeout)?;
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
    // Pointer to the ngx_resolver_ctx_t. Resolution constructs this with
    // ngx_resolver_name_start in the constructor, and is responsible for
    // freeing it, with ngx_resolver_name_done, once it is no longer needed -
    // this happens in either the callback handler, or the drop impl. Calling
    // ngx_resolver_name_done before the callback fires ensure nginx does not
    // ever call the callback.
    ctx: Option<NonNull<ngx_resolver_ctx_t>>,
}

impl<'a> Resolution<'a> {
    fn new(
        name: &ngx_str_t,
        pool: &'a Pool,
        resolver: NonNull<ngx_resolver_t>,
        timeout: ngx_msec_t,
    ) -> Result<Pin<Box<Self>>, Error> {
        let mut ctx = unsafe {
            // Start a new resolver context. This implementation currently
            // passes a null for the second argument `temp`. A non-null `temp`
            // provides a fast, non-callback-based path for immediately
            // returning an addr iff `temp` contains a name which is textual
            // form of an addr.
            let ctx = ngx_resolve_start(resolver.as_ptr(), core::ptr::null_mut());
            NonNull::new(ctx).ok_or(Error::AllocationFailed)?
        };

        // Create a pinned Resolution on the heap, so that we can make
        // a stable pointer to the Resolution struct.
        let mut this = Pin::new(Box::new(Resolution {
            complete: None,
            waker: None,
            pool,
            ctx: Some(ctx),
        }));

        {
            // Set up the ctx with everything the resolver needs to resolve a
            // name, and the handler callback which is called on completion.
            let ctx: &mut ngx_resolver_ctx_t = unsafe { ctx.as_mut() };
            ctx.name = *name;
            ctx.timeout = timeout;
            ctx.set_cancelable(1);
            ctx.handler = Some(Self::handler);
            // Safety: Self::handler, Future::poll, and Drop::drop will have
            // access to &mut Resolution. Nginx is single-threaded and we are
            // assured only one of those is on the stack at a time, except if
            // Self::handler wakes a task which polls or drops the Future,
            // which it only does after use of &mut Resolution is complete.
            let ptr: &mut Resolution = unsafe { Pin::into_inner_unchecked(this.as_mut()) };
            ctx.data = ptr as *mut Resolution as *mut c_void;
        }

        // Start name resolution using the ctx. If the name is in the dns
        // cache, the handler may get called from this stack. Otherwise, it
        // will be called later by nginx when it gets a dns response or a
        // timeout.
        let ret = unsafe { ngx_resolve_name(ctx.as_ptr()) };
        if ret != 0 {
            return Err(Error::Resolver(
                ResolverError::try_from(ret).expect("nonzero, checked above"),
                name.to_string(),
            ));
        }

        Ok(this)
    }

    // Nginx will call this handler when name resolution completes. If the
    // result is cached, this could be
    unsafe extern "C" fn handler(ctx: *mut ngx_resolver_ctx_t) {
        let mut data = unsafe { NonNull::new_unchecked((*ctx).data as *mut Resolution) };
        let this: &mut Resolution = unsafe { data.as_mut() };
        this.complete = Some(Self::resolve_result(ctx, this.pool));

        let mut ctx = this.ctx.take().expect("ctx must be present");
        unsafe { nginx_sys::ngx_resolve_name_done(ctx.as_mut()) };

        // Wake last, after all use of &mut Resolution, because wake may
        // poll Resolution future on current stack.
        if let Some(waker) = this.waker.take() {
            waker.wake();
        }
    }

    /// Take the results in a ctx and make an owned copy as a
    /// Result<Vec<ngx_addr_t>, Error>, where both the Vec and internals of
    /// the ngx_addr_t are allocated on the given Pool
    fn resolve_result(ctx: *mut ngx_resolver_ctx_t, pool: &Pool) -> Res {
        let ctx = unsafe { ctx.as_ref().unwrap() };
        let s = ctx.state;
        if s != 0 {
            return Err(Error::Resolver(
                ResolverError::try_from(s).expect("nonzero, checked above"),
                ctx.name.to_string(),
            ));
        }
        if ctx.addrs.is_null() {
            Err(Error::AllocationFailed)?;
        }
        let mut out = Vec::new();
        for i in 0..ctx.naddrs {
            out.push(Self::copy_resolved_addr(unsafe { ctx.addrs.add(i) }, pool)?);
        }
        Ok(out)
    }

    /// Take the contents of an ngx_resolver_addr_t and make an owned copy as
    /// an ngx_addr_t, using the Pool for allocation of the internals.
    fn copy_resolved_addr(
        addr: *mut nginx_sys::ngx_resolver_addr_t,
        pool: &Pool,
    ) -> Result<ngx_addr_t, Error> {
        let addr = NonNull::new(addr).ok_or(Error::Unexpected(
            "null ngx_resolver_addr_t in ngx_resolver_ctx_t.addrs".to_string(),
        ))?;
        let addr = unsafe { addr.as_ref() };

        let sockaddr = pool.alloc(addr.socklen as usize) as *mut nginx_sys::sockaddr;
        if sockaddr.is_null() {
            Err(Error::AllocationFailed)?;
        }
        unsafe {
            addr.sockaddr
                .cast::<u8>()
                .copy_to_nonoverlapping(sockaddr.cast(), addr.socklen as usize)
        };

        let name = unsafe {
            ngx_str_t::from_bytes(pool.as_ref() as *const _ as *mut _, addr.name.as_bytes())
        }
        .ok_or(Error::AllocationFailed)?;

        Ok(ngx_addr_t {
            sockaddr,
            socklen: addr.socklen,
            name,
        })
    }
}

impl<'a> core::future::Future for Resolution<'a> {
    type Output = Result<Vec<ngx_addr_t>, Error>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut();
        // The handler populates this.complete, and we consume it here:
        match this.complete.take() {
            Some(res) => Poll::Ready(res),
            None => {
                // If the handler has not yet fired, populate the waker field,
                // which the handler will consume:
                match &mut self.waker {
                    None => {
                        self.waker = Some(cx.waker().clone());
                    }
                    Some(w) => w.clone_from(cx.waker()),
                }
                Poll::Pending
            }
        }
    }
}

impl<'a> Drop for Resolution<'a> {
    fn drop(&mut self) {
        // ctx is taken and freed if the Resolution reaches the handler
        // callback, but if dropped before that callback, this will cancel any
        // ongoing work as well as free the ctx memory.
        if let Some(mut ctx) = self.ctx.take() {
            unsafe {
                nginx_sys::ngx_resolve_name_done(ctx.as_mut());
            }
        }
    }
}
