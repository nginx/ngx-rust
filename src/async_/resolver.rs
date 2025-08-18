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
use core::ptr::NonNull;

use crate::{
    allocator::Box,
    collections::Vec,
    core::Pool,
    ffi::{
        ngx_addr_t, ngx_msec_t, ngx_resolve_name, ngx_resolve_start, ngx_resolver_ctx_t,
        ngx_resolver_t, ngx_str_t,
    },
};
use futures_channel::oneshot::{channel, Sender};
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

struct ResCtx<'a> {
    ctx: Option<*mut ngx_resolver_ctx_t>,
    sender: Option<Sender<Res>>,
    pool: &'a Pool,
}

impl Drop for ResCtx<'_> {
    fn drop(&mut self) {
        if let Some(ctx) = self.ctx.take() {
            unsafe {
                nginx_sys::ngx_resolve_name_done(ctx);
            }
        }
    }
}

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

    let name =
        unsafe { ngx_str_t::from_bytes(pool.as_ref() as *const _ as *mut _, addr.name.as_bytes()) }
            .ok_or(Error::AllocationFailed)?;

    Ok(ngx_addr_t {
        sockaddr,
        socklen: addr.socklen,
        name,
    })
}

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
        unsafe {
            let ctx: *mut ngx_resolver_ctx_t =
                ngx_resolve_start(self.resolver.as_ptr(), core::ptr::null_mut());
            if ctx.is_null() {
                Err(Error::AllocationFailed)?
            }
            if ctx as isize == -1 {
                Err(Error::NoResolver)?
            }

            let (sender, receiver) = channel::<Res>();
            let rctx = Box::new(ResCtx {
                ctx: Some(ctx),
                sender: Some(sender),
                pool,
            });

            (*ctx).name = *name;
            (*ctx).timeout = self.timeout;
            (*ctx).set_cancelable(1);
            (*ctx).handler = Some(Self::resolve_handler);
            (*ctx).data = Box::into_raw(rctx) as *mut c_void;

            let ret = ngx_resolve_name(ctx);
            if ret != 0 {
                Err(Error::Resolver(
                    ResolverError::try_from(ret).expect("nonzero, checked above"),
                    name.to_string(),
                ))?;
            }

            receiver
                .await
                .map_err(|_| Error::Resolver(ResolverError::TimedOut, name.to_string()))?
        }
    }

    unsafe extern "C" fn resolve_handler(ctx: *mut ngx_resolver_ctx_t) {
        let mut rctx = Box::into_inner(unsafe { Box::from_raw((*ctx).data as *mut ResCtx) });
        rctx.ctx.take();
        if let Some(sender) = rctx.sender.take() {
            let _ = sender.send(Self::resolve_result(ctx, rctx.pool));
        }
        unsafe { nginx_sys::ngx_resolve_name_done(ctx) };
    }

    fn resolve_result(ctx: *mut ngx_resolver_ctx_t, pool: &Pool) -> Res {
        let ctx = unsafe { ctx.as_ref().unwrap() };
        let s = ctx.state;
        if s != 0 {
            Err(Error::Resolver(
                ResolverError::try_from(s).expect("nonzero, checked above"),
                ctx.name.to_string(),
            ))?;
        }
        if ctx.addrs.is_null() {
            Err(Error::AllocationFailed)?;
        }
        let mut out = Vec::new();
        for i in 0..ctx.naddrs {
            out.push(copy_resolved_addr(unsafe { ctx.addrs.add(i) }, pool)?);
        }
        Ok(out)
    }
}
