//! Collection types.
//!
//! This module provides common collection types, mostly implemented as wrappers over the
//! corresponding NGINX types.

#[cfg(feature = "alloc")]
pub use allocator_api2::{
    collections::{TryReserveError, TryReserveErrorKind},
    vec, // reexport both the module and the macro
    vec::Vec,
};

pub use rbtree::RbTreeMap;

pub mod rbtree;
