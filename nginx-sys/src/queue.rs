use core::ptr;

use crate::bindings::ngx_queue_t;

/// Get a reference to the beginning of a queue node data structure,
/// considering the queue field offset in it.
///
/// # Safety
///
/// `$q` must be a valid pointer to the field `$link` in the struct `$type`
#[macro_export]
macro_rules! ngx_queue_data {
    ($q:expr, $type:path, $link:ident) => {
        $q.byte_sub(::core::mem::offset_of!($type, $link)).cast::<$type>()
    };
}

/// Initializes the queue head before use.
///
/// # Safety
///
/// `q` must be a valid pointer to [ngx_queue_t].
#[inline]
pub unsafe fn ngx_queue_init(q: *mut ngx_queue_t) {
    (*q).prev = q;
    (*q).next = q;
}

/// Returns `true` if the queue contains no elements.
///
/// # Safety
///
/// `q` must be a valid pointer to [ngx_queue_t], initialized with [ngx_queue_init].
#[inline]
pub unsafe fn ngx_queue_empty(q: *const ngx_queue_t) -> bool {
    q == (*q).prev
}

/// Inserts a new node after the current.
///
/// # Safety
///
/// Both `q` and `x` must be valid pointers to [ngx_queue_t]
#[inline]
pub unsafe fn ngx_queue_insert_after(q: *mut ngx_queue_t, x: *mut ngx_queue_t) {
    (*x).next = (*q).next;
    (*(*x).next).prev = x;
    (*x).prev = q;
    (*q).next = x;
}

/// Inserts a new node before the current.
///
/// # Safety
///
/// Both `q` and `x` must be valid pointers to [ngx_queue_t].
#[inline]
pub unsafe fn ngx_queue_insert_before(q: *mut ngx_queue_t, x: *mut ngx_queue_t) {
    (*x).prev = (*q).prev;
    (*(*x).prev).next = x;
    (*x).next = q;
    (*q).prev = x;
}

/// Removes a node from the queue.
///
/// # Safety
///
/// `q` must be a valid pointer to an [ngx_queue_t] node.
#[inline]
pub unsafe fn ngx_queue_remove(q: *mut ngx_queue_t) {
    (*(*q).next).prev = (*q).prev;
    (*(*q).prev).next = (*q).next;
    (*q).prev = ptr::null_mut();
    (*q).next = ptr::null_mut();
}

/// Splits a queue at a node, returning the queue tail in a separate queue.
///
/// # Safety
///
/// `h` must be a valid pointer to a head queue node.
/// `q` must be a node in the queue `h`.
/// `n` must be a valid pointer to [ngx_queue_t].
#[inline]
pub unsafe fn ngx_queue_split(h: *mut ngx_queue_t, q: *mut ngx_queue_t, n: *mut ngx_queue_t) {
    (*n).prev = (*h).prev;
    (*(*n).prev).next = n;
    (*n).next = q;
    (*h).prev = (*q).prev;
    (*(*h).prev).next = h;
    (*q).prev = n;
}

/// Adds a second queue to the first queue.
///
/// # Safety
///
/// Both `h` and `n` must be valid pointers to queue heads, initialized with [ngx_queue_init].
/// `n` will be left in invalid state, pointing to the subrange of `h` without back references.
#[inline]
pub unsafe fn ngx_queue_add(h: *mut ngx_queue_t, n: *mut ngx_queue_t) {
    (*(*h).prev).next = (*n).next;
    (*(*n).next).prev = (*h).prev;
    (*h).prev = (*n).prev;
    (*(*h).prev).next = h;
}

impl ngx_queue_t {
    /// Returns `true` if the queue contains no elements.
    pub fn is_empty(&self) -> bool {
        unsafe { ngx_queue_empty(self) }
    }
}

impl Default for ngx_queue_t {
    fn default() -> ngx_queue_t {
        ngx_queue_t {
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::boxed::Box;

    use super::*;

    struct TestData {
        value: usize,
        queue: ngx_queue_t,
    }

    impl TestData {
        pub fn new(value: usize) -> *mut Self {
            // We should be using `ngx_pool_t` here, but that is not possible without linking to
            // the nginx
            let mut x = Box::new(Self {
                value,
                queue: Default::default(),
            });
            unsafe { ngx_queue_init(ptr::addr_of_mut!(x.queue)) };
            Box::into_raw(x)
        }

        pub unsafe fn free(x: *mut Self) {
            let _ = Box::from_raw(x);
        }
    }

    impl Drop for TestData {
        fn drop(&mut self) {
            if !self.queue.next.is_null() && !self.queue.is_empty() {
                unsafe { ngx_queue_remove(ptr::addr_of_mut!(self.queue)) };
            }
        }
    }

    struct Iter {
        h: *mut ngx_queue_t,
        q: *mut ngx_queue_t,
        next: fn(*mut ngx_queue_t) -> *mut ngx_queue_t,
    }

    impl Iter {
        pub fn new(h: *mut ngx_queue_t) -> Self {
            let next = |x: *mut ngx_queue_t| unsafe { (*x).next };
            Self { h, q: next(h), next }
        }

        pub fn new_reverse(h: *mut ngx_queue_t) -> Self {
            let next = |x: *mut ngx_queue_t| unsafe { (*x).prev };
            Self { h, q: next(h), next }
        }
    }

    impl Iterator for Iter {
        type Item = *mut ngx_queue_t;

        fn next(&mut self) -> Option<Self::Item> {
            if self.h == self.q {
                return None;
            }

            let item = self.q;
            self.q = (self.next)(self.q);
            Some(item)
        }
    }

    #[test]
    fn test_queue() {
        fn value(q: *mut ngx_queue_t) -> usize {
            unsafe { (*ngx_queue_data!(q, TestData, queue)).value }
        }

        // Check forward and reverse iteration
        fn cmp(h: *mut ngx_queue_t, other: &[usize]) -> bool {
            Iter::new(h).map(value).eq(other.iter().cloned())
                && Iter::new_reverse(h).map(value).eq(other.iter().rev().cloned())
        }

        // Note how this test does not use references or borrows to avoid triggering UBs
        // detectable by Miri. This does not mean that the code is safe or sound.
        unsafe {
            // Initialize and fill the queue

            let mut h1 = ngx_queue_t::default();
            ngx_queue_init(ptr::addr_of_mut!(h1));

            let mut h2 = ngx_queue_t::default();
            ngx_queue_init(ptr::addr_of_mut!(h2));

            for i in 1..=5 {
                let elem = TestData::new(i);
                ngx_queue_insert_before(ptr::addr_of_mut!(h1), ptr::addr_of_mut!((*elem).queue));

                let elem = TestData::new(i);
                ngx_queue_insert_after(ptr::addr_of_mut!(h2), ptr::addr_of_mut!((*elem).queue));
            }

            // Iterate and test the values

            assert!(cmp(ptr::addr_of_mut!(h1), &[1, 2, 3, 4, 5]));
            assert!(cmp(ptr::addr_of_mut!(h2), &[5, 4, 3, 2, 1]));

            // Move nodes from h2 to h1

            // h2 still points to the subrange of h1 after this operation
            ngx_queue_add(ptr::addr_of_mut!(h1), ptr::addr_of_mut!(h2));

            assert!(cmp(ptr::addr_of_mut!(h1), &[1, 2, 3, 4, 5, 5, 4, 3, 2, 1]));

            ngx_queue_split(ptr::addr_of_mut!(h1), (*h2.next).next, ptr::addr_of_mut!(h2));

            assert!(cmp(ptr::addr_of_mut!(h1), &[1, 2, 3, 4, 5, 5]));
            assert!(cmp(ptr::addr_of_mut!(h2), &[4, 3, 2, 1]));

            // Cleanup

            for q in Iter::new(ptr::addr_of_mut!(h1)) {
                let td = ngx_queue_data!(q, TestData, queue);
                TestData::free(td);
            }
            assert!(h1.is_empty());

            for q in Iter::new(ptr::addr_of_mut!(h2)) {
                let td = ngx_queue_data!(q, TestData, queue);
                TestData::free(td);
            }
            assert!(h2.is_empty());
        };
    }
}
