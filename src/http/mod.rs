#[cfg(feature = "async")]
mod async_request;
// #[cfg(feature = "async")]
// mod async_subrequest;

mod conf;
mod module;
mod request;
mod request_context;
mod status;
mod upstream;

/// HTTP subrequest builder and handler.
#[cfg(feature = "alloc")]
pub mod subrequest;

#[cfg(feature = "async")]
pub use async_request::*;
// #[cfg(feature = "async")]
// pub use async_subrequest::*;

pub use conf::*;
pub use module::*;
pub use request::*;
pub use request_context::*;
pub use status::*;
