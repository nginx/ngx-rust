// TODO: not sure how to make no_std compatible? spin::Once could replace OnceLock, but
// crossbeam_channel needs std afaik.
extern crate std;
use std::sync::OnceLock;

use core::future::Future;

use async_task::Runnable;
pub use async_task::Task;
use crossbeam_channel::{unbounded, Receiver, Sender};
use nginx_sys::{ngx_event_actions, ngx_event_t, ngx_thread_tid};

use crate::log::ngx_cycle_log;
use crate::ngx_log_debug;

// NOTE: first schedule will always be indirected via ngx_notify, it will fill MAIN_TID
// alternatives:
// - have modules call something in init to establish main thread id
// - use std::process::id().try_into().unwrap(), but only on Linux iirc?
static MAIN_TID: OnceLock<u64> = OnceLock::new();

#[inline]
fn current_tid_u64() -> u64 {
    unsafe { ngx_thread_tid() as u64 }
}

#[inline]
fn on_event_thread() -> bool {
    MAIN_TID
        .get()
        .is_some_and(|&main| main == current_tid_u64())
}

extern "C" fn notify_handler(_ev: *mut ngx_event_t) {
    let _ = MAIN_TID.set(current_tid_u64());
    let scheduler = scheduler();
    let mut cnt = 0;
    while let Ok(r) = scheduler.rx.try_recv() {
        r.run();
        cnt += 1;
    }
    ngx_log_debug!(
        ngx_cycle_log().as_ptr(),
        "async: notify_handler processed {cnt} items"
    );
}

fn notify() {
    ngx_log_debug!(ngx_cycle_log().as_ptr(), "async: ngx_notify");
    unsafe {
        ngx_event_actions.notify.expect("ngx_notify")(Some(notify_handler));
    }
}

// NOTE: This could be a std::sync::mpsc::sync_channel(1) actually, but that's not Sync so it would
// require rx to be Mutex<Receiver<Runnable>> or similar.
// Alternatively, one could `unsafe impl Sync` on a wrapper around Receiver, as one can be sure
// that it will only ever be called thread-safely from the event loop (the Sender is Clone and
// doesn't have this issue).
struct Scheduler {
    rx: Receiver<Runnable>,
    tx: Sender<Runnable>,
}

impl Scheduler {
    fn new() -> Self {
        let (tx, rx) = unbounded();
        Scheduler { tx, rx }
    }

    fn schedule(&self, runnable: Runnable) {
        // are we on main thread just .run()…
        if on_event_thread() {
            runnable.run();
        } else {
            // …otherwise we were called from some other thread, e.g. io handler:
            // ngx_notify to interrupt epoll and move it into the event loop
            self.tx.send(runnable).expect("send");
            notify();
        }
    }
}

static SCHEDULER: OnceLock<Scheduler> = OnceLock::new();

fn scheduler() -> &'static Scheduler {
    SCHEDULER.get_or_init(Scheduler::new)
}

fn schedule(runnable: Runnable) {
    let scheduler = scheduler();
    scheduler.schedule(runnable);
}

/// Creates a new task running on the NGINX event loop.
pub fn spawn<F, T>(future: F) -> Task<T>
where
    F: Future<Output = T> + 'static,
    T: 'static,
{
    ngx_log_debug!(ngx_cycle_log().as_ptr(), "async: spawning new task");
    // safe alternative: spawn_local, but this would check tid twice needlessly
    let (runnable, task) = unsafe { async_task::spawn_unchecked(future, schedule) };
    runnable.schedule();
    task
}
