use core::ptr;

use crate::{
    ngx_current_msec, ngx_event_t, ngx_event_timer_rbtree, ngx_msec_t, ngx_queue_insert_before, ngx_queue_remove,
    ngx_queue_t, ngx_rbtree_delete, ngx_rbtree_insert, NGX_TIMER_LAZY_DELAY,
};

/// Sets a timeout for an event.
///
/// # Safety
///
///`ev` must be a valid pointer to an `ngx_event_t`.
#[inline]
pub unsafe fn ngx_add_timer(ev: *mut ngx_event_t, timer: ngx_msec_t) {
    let key: ngx_msec_t = ngx_current_msec.wrapping_add(timer);

    if (*ev).timer_set() != 0 {
        /*
         * Use a previous timer value if difference between it and a new
         * value is less than NGX_TIMER_LAZY_DELAY milliseconds: this allows
         * to minimize the rbtree operations for fast connections.
         */
        if key.abs_diff((*ev).timer.key) < NGX_TIMER_LAZY_DELAY as _ {
            return;
        }

        ngx_del_timer(ev);
    }

    (*ev).timer.key = key;

    ngx_rbtree_insert(
        ptr::addr_of_mut!(ngx_event_timer_rbtree),
        ptr::addr_of_mut!((*ev).timer),
    );

    (*ev).set_timer_set(1);
}

/// Deletes a previously set timeout.
///
/// # Safety
///
/// `ev` must be a valid pointer to an `ngx_event_t`, previously armed with [ngx_add_timer].
#[inline]
pub unsafe fn ngx_del_timer(ev: *mut ngx_event_t) {
    ngx_rbtree_delete(
        ptr::addr_of_mut!(ngx_event_timer_rbtree),
        ptr::addr_of_mut!((*ev).timer),
    );

    (*ev).timer.left = ptr::null_mut();
    (*ev).timer.right = ptr::null_mut();
    (*ev).timer.parent = ptr::null_mut();

    (*ev).set_timer_set(0);
}

/// Post the event `ev` to the post queue `q`.
///
/// # Safety
///
/// `ev` must be a valid pointer to an `ngx_event_t`.
/// `q` is a valid pointer to a queue head.
#[inline]
pub unsafe fn ngx_post_event(ev: *mut ngx_event_t, q: *mut ngx_queue_t) {
    if (*ev).posted() == 0 {
        (*ev).set_posted(1);
        ngx_queue_insert_before(q, ptr::addr_of_mut!((*ev).queue));
    }
}

/// Deletes the event `ev` from the queue it's currently posted in.
///
/// # Safety
///
/// `ev` must be a valid pointer to an `ngx_event_t`.
/// `ev.queue` is initialized with `ngx_queue_init`.
#[inline]
pub unsafe fn ngx_delete_posted_event(ev: *mut ngx_event_t) {
    (*ev).set_posted(0);
    ngx_queue_remove(ptr::addr_of_mut!((*ev).queue));
}
