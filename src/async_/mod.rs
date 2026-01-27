//! Async runtime and set of utilities on top of the NGINX event loop.
pub use self::sleep::{Sleep, sleep};
pub use self::spawn::{Task, spawn};

pub mod resolver;

mod sleep;
mod spawn;
