//! Async runtime and set of utilities on top of the NGINX event loop.
pub use self::sleep::{sleep, Sleep};
pub use self::spawn::{spawn, Task};

mod sleep;
mod spawn;
