use alloc::collections::vec_deque::VecDeque;
use core::cell::UnsafeCell;
use core::future::Future;
use core::mem;
use core::ptr::{self, NonNull};

pub use async_task::Task;
use async_task::{Runnable, ScheduleInfo, WithInfo};
use nginx_sys::{
    ngx_del_timer, ngx_delete_posted_event, ngx_event_t, ngx_post_event, ngx_posted_next_events,
};

use crate::log::ngx_cycle_log;
use crate::{ngx_container_of, ngx_log_debug};

static SCHEDULER: Scheduler = Scheduler::new();

struct Scheduler(UnsafeCell<SchedulerInner>);

// SAFETY: Scheduler must only be used from the main thread of a worker process.
unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

impl Scheduler {
    const fn new() -> Self {
        Self(SchedulerInner::new())
    }

    pub fn schedule(&self, runnable: Runnable) {
        // SAFETY: the cell is not empty, and we have exclusive access due to being a
        // single-threaded application.
        let inner = unsafe { &mut *UnsafeCell::raw_get(&self.0) };
        inner.send(runnable)
    }
}

#[repr(C)]
struct SchedulerInner {
    _ident: [usize; 4], // `ngx_event_ident` compatibility
    event: ngx_event_t,
    queue: VecDeque<Runnable>,
}

impl SchedulerInner {
    const fn new() -> UnsafeCell<Self> {
        let mut event: ngx_event_t = unsafe { mem::zeroed() };
        event.handler = Some(Self::scheduler_event_handler);

        UnsafeCell::new(Self {
            _ident: [
                0, 0, 0, 0x4153594e, // ASYN
            ],
            event,
            queue: VecDeque::new(),
        })
    }

    pub fn send(&mut self, runnable: Runnable) {
        // Cached `ngx_cycle.log` can be invalidated when reloading configuration in a single
        // process mode. Update `log` every time to avoid using stale log pointer.
        self.event.log = ngx_cycle_log().as_ptr();

        // While this event is not used as a timer at the moment, we still want to ensure that it is
        // compatible with `ngx_event_ident`.
        if self.event.data.is_null() {
            self.event.data = ptr::from_mut(self).cast();
        }

        // FIXME: VecDeque::push could panic on an allocation failure, switch to a datastructure
        // which will not and propagate the failure.
        self.queue.push_back(runnable);
        unsafe { ngx_post_event(&mut self.event, ptr::addr_of_mut!(ngx_posted_next_events)) }
    }

    /// This event handler is called by ngx_event_process_posted at the end of
    /// ngx_process_events_and_timers.
    extern "C" fn scheduler_event_handler(ev: *mut ngx_event_t) {
        let mut runnables = {
            // SAFETY:
            // This handler always receives a non-null pointer to an event embedded into a
            // UnsafeCell<SchedulerInner> instance. We modify the contents of the `UnsafeCell`,
            // but we ensured that:
            //  - we access the cell correctly, as documented in
            //    https://doc.rust-lang.org/stable/std/cell/struct.UnsafeCell.html#memory-layout
            //  - the access is unique due to being single-threaded
            //  - the reference is dropped before we start processing queued runnables.
            let cell: NonNull<UnsafeCell<Self>> =
                unsafe { ngx_container_of!(NonNull::new_unchecked(ev), Self, event).cast() };
            let this = unsafe { &mut *UnsafeCell::raw_get(cell.as_ptr()) };

            ngx_log_debug!(
                this.event.log,
                "async: processing {} deferred wakeups",
                this.queue.len()
            );

            // Move runnables to a new queue to avoid borrowing from the SchedulerInner and limit
            // processing to already queued wakeups. This ensures that we correctly handle tasks
            // that keep scheduling themselves (e.g. using yield_now() in a loop).
            // We can't use drain() as it borrows from self and breaks aliasing rules.
            mem::take(&mut this.queue)
        };

        for runnable in runnables.drain(..) {
            runnable.run();
        }
    }
}

impl Drop for SchedulerInner {
    fn drop(&mut self) {
        if self.event.posted() != 0 {
            unsafe { ngx_delete_posted_event(&mut self.event) };
        }

        if self.event.timer_set() != 0 {
            unsafe { ngx_del_timer(&mut self.event) };
        }
    }
}

fn schedule(runnable: Runnable, info: ScheduleInfo) {
    if info.woken_while_running {
        SCHEDULER.schedule(runnable);
        ngx_log_debug!(
            ngx_cycle_log().as_ptr(),
            "async: task scheduled while running"
        );
    } else {
        runnable.run();
    }
}

/// Creates a new task running on the NGINX event loop.
pub fn spawn<F, T>(future: F) -> Task<T>
where
    F: Future<Output = T> + 'static,
    T: 'static,
{
    ngx_log_debug!(ngx_cycle_log().as_ptr(), "async: spawning new task");
    let scheduler = WithInfo(schedule);
    // Safety: single threaded embedding takes care of send/sync requirements for future and
    // scheduler. Future and scheduler are both 'static.
    let (runnable, task) = unsafe { async_task::spawn_unchecked(future, scheduler) };
    runnable.schedule();
    task
}
