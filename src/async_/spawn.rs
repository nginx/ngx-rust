extern crate std;

use core::sync::atomic::{AtomicI64, Ordering};

use core::future::Future;
use std::sync::OnceLock;

pub use async_task::Task;
use async_task::{Runnable, ScheduleInfo, WithInfo};
use crossbeam_channel::{unbounded, Receiver, Sender};
use nginx_sys::{ngx_event_t, ngx_thread_tid};

use crate::log::ngx_cycle_log;
use crate::ngx_log_debug;

#[cfg(not(feature = "async_eventfd"))]
mod sigio {
    extern crate std;

    use core::mem;
    use std::{process::id, sync::OnceLock};

    use nginx_sys::{kill, ngx_event_t, ngx_post_event, ngx_posted_next_events, SIGIO};

    use super::async_handler;
    use crate::log::ngx_cycle_log;
    use crate::ngx_log_debug;

    struct NotifyContext {
        ev: ngx_event_t,
    }
    static mut CTX: NotifyContext = NotifyContext {
        ev: unsafe { mem::zeroed() },
    };

    static INIT: OnceLock<()> = OnceLock::new();

    fn ensure_init() {
        let _ = INIT.get_or_init(|| {
            #[allow(clippy::deref_addrof)]
            let ctx = unsafe { &mut *&raw mut CTX };

            ctx.ev.log = ngx_cycle_log().as_ptr();
            ctx.ev.handler = Some(async_handler);
        });
    }

    pub(crate) fn notify() {
        ensure_init();

        unsafe { ngx_post_event(&raw mut CTX.ev, &raw mut ngx_posted_next_events) };

        let rc = unsafe { kill(id().try_into().unwrap(), SIGIO.try_into().unwrap()) };
        if rc != 0 {
            panic!("async: kill rc={rc}");
        }

        ngx_log_debug!(ngx_cycle_log().as_ptr(), "async: notified (SIGIO)");
    }

    // called from async_handler
    pub(crate) fn confirm_notification() {
        // nop
    }
}
#[cfg(not(feature = "async_eventfd"))]
use sigio::*;

#[cfg(feature = "async_eventfd")]
mod eventfd {
    extern crate std;

    use core::ffi::c_void;
    use core::mem;
    use std::sync::OnceLock;

    use nginx_sys::{
        eventfd, ngx_connection_t, ngx_event_actions, ngx_event_t, read, write, EFD_CLOEXEC,
        EFD_NONBLOCK, EPOLL_EVENTS_EPOLLET, EPOLL_EVENTS_EPOLLIN, EPOLL_EVENTS_EPOLLRDHUP, NGX_OK,
    };

    use super::async_handler;
    use crate::log::ngx_cycle_log;
    use crate::ngx_log_debug;

    #[cfg(not(ngx_feature = "have_eventfd"))]
    compile_error!("feature async_eventfd requires eventfd(), NGX_HAVE_EVENTFD");

    struct NotifyContext {
        c: ngx_connection_t,
        rev: ngx_event_t,
        wev: ngx_event_t,
        fd: i32,
    }
    static mut CTX: NotifyContext = NotifyContext {
        c: unsafe { mem::zeroed() },
        rev: unsafe { mem::zeroed() },
        wev: unsafe { mem::zeroed() },
        fd: -1,
    };

    static INIT: OnceLock<()> = OnceLock::new();

    extern "C" fn _dummy_write_handler(_ev: *mut ngx_event_t) {}

    fn ensure_init() {
        let _ = INIT.get_or_init(|| {
            let fd = unsafe { eventfd(0, (EFD_NONBLOCK | EFD_CLOEXEC).try_into().unwrap()) };

            if fd == -1 {
                panic!("async: eventfd = -1");
            }

            #[allow(clippy::deref_addrof)]
            let ctx = unsafe { &mut *&raw mut CTX };

            let log = ngx_cycle_log().as_ptr();

            ctx.c.log = log;
            ctx.c.fd = fd;
            ctx.c.read = &raw mut ctx.rev;
            ctx.c.write = &raw mut ctx.wev;

            ctx.rev.log = log;
            ctx.rev.data = (&raw mut ctx.c).cast();
            ctx.rev.set_active(1);
            ctx.rev.handler = Some(async_handler);

            ctx.wev.log = log;
            ctx.wev.data = (&raw mut ctx.c).cast();
            ctx.wev.handler = Some(_dummy_write_handler); // can't be null
            let rc = unsafe {
                ngx_event_actions.add.unwrap()(
                    &raw mut ctx.rev,
                    (EPOLL_EVENTS_EPOLLIN | EPOLL_EVENTS_EPOLLRDHUP) as isize,
                    EPOLL_EVENTS_EPOLLET as usize,
                )
            };
            if rc != NGX_OK as isize {
                panic!("async: ngx_add_event rc={rc}");
            }

            ctx.fd = fd;
        });
    }

    pub(crate) fn notify() {
        ensure_init();

        let val: u64 = 1;
        let ptr = &val as *const u64 as *const c_void;
        let res = unsafe { write(CTX.fd, ptr, core::mem::size_of::<u64>()) };
        if res != core::mem::size_of::<u64>() as isize {
            panic!("eventfd write failed: {res}");
        }

        ngx_log_debug!(ngx_cycle_log().as_ptr(), "async: notified (eventfd)");
    }

    // called from async_handler
    pub(crate) fn confirm_notification() {
        let mut buf: u64 = 0;
        let ptr = &mut buf as *mut u64 as *mut c_void;
        let _ = unsafe { read(CTX.fd, ptr, core::mem::size_of::<u64>()) };
    }
}

#[cfg(feature = "async_eventfd")]
use eventfd::*;

static MAIN_TID: AtomicI64 = AtomicI64::new(-1);

#[inline]
fn on_event_thread() -> bool {
    let main_tid = MAIN_TID.load(Ordering::Relaxed);
    let tid: i64 = unsafe { ngx_thread_tid().into() };
    main_tid == tid
}

extern "C" fn async_handler(_ev: *mut ngx_event_t) {
    // initialize MAIN_TID on first execution
    let tid = unsafe { ngx_thread_tid().into() };
    let _ = MAIN_TID.compare_exchange(-1, tid, Ordering::Relaxed, Ordering::Relaxed);

    confirm_notification();

    let scheduler = scheduler();

    if scheduler.rx.is_empty() {
        return;
    }
    let mut cnt = 0;
    while let Ok(r) = scheduler.rx.try_recv() {
        r.run();
        cnt += 1;
    }
    ngx_log_debug!(ngx_cycle_log().as_ptr(), "async: processed {cnt} items");
}

struct Scheduler {
    rx: Receiver<Runnable>,
    tx: Sender<Runnable>,
}

impl Scheduler {
    fn new() -> Self {
        let (tx, rx) = unbounded();
        Scheduler { tx, rx }
    }

    fn schedule(&self, runnable: Runnable, info: ScheduleInfo) {
        let oet = on_event_thread();
        // If we are on the event loop thread it's safe to simply run the Runnable, otherwise we
        // enqueue the Runnable, post our event, and notify. The event handler then runs the
        // Runnable on the event loop thread.
        //
        // If woken_while_running, it indicates that a task has yielded itself to the Scheduler.
        // Force round-trip via queue to limit reentrancy.
        if oet && !info.woken_while_running {
            runnable.run();
        } else {
            self.tx.send(runnable).expect("send");

            notify();
        }
    }
}

static SCHEDULER: OnceLock<Scheduler> = OnceLock::new();

fn scheduler() -> &'static Scheduler {
    SCHEDULER.get_or_init(Scheduler::new)
}

fn schedule(runnable: Runnable, info: ScheduleInfo) {
    let scheduler = scheduler();
    scheduler.schedule(runnable, info);
}

/// Creates a new task running on the NGINX event loop.
pub fn spawn<F, T>(future: F) -> Task<T>
where
    F: Future<Output = T> + 'static,
    T: 'static,
{
    ngx_log_debug!(ngx_cycle_log().as_ptr(), "async: spawning new task");
    let (runnable, task) = unsafe { async_task::spawn_unchecked(future, WithInfo(schedule)) };
    runnable.schedule();
    task
}
