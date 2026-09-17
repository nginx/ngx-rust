use core::{ptr, slice};

use crate::ffi::*;

/// The `Buffer` trait provides methods for working with an nginx buffer (`ngx_buf_t`).
///
/// See <https://nginx.org/en/docs/dev/development_guide.html#buffer>
pub trait Buffer {
    /// Returns a raw pointer to the underlying `ngx_buf_t` of the buffer.
    fn as_ngx_buf(&self) -> *const ngx_buf_t;

    /// Returns a mutable raw pointer to the underlying `ngx_buf_t` of the buffer.
    fn as_ngx_buf_mut(&mut self) -> *mut ngx_buf_t;

    /// Returns the buffer contents as a byte slice.
    fn as_bytes(&self) -> &[u8] {
        let buf = self.as_ngx_buf();
        unsafe { slice::from_raw_parts((*buf).pos, self.len()) }
    }

    /// Returns the length of the buffer contents.
    fn len(&self) -> usize {
        let buf = self.as_ngx_buf();
        unsafe {
            let pos = (*buf).pos;
            let last = (*buf).last;
            assert!(last >= pos);
            usize::wrapping_sub(last as _, pos as _)
        }
    }

    /// Returns `true` if the buffer is empty, i.e., it has zero length.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sets the `last_buf` flag of the buffer.
    ///
    /// # Arguments
    ///
    /// * `last` - A boolean indicating whether the buffer is the last buffer in a request.
    fn set_last_buf(&mut self, last: bool) {
        let buf = self.as_ngx_buf_mut();
        unsafe {
            (*buf).set_last_buf(if last { 1 } else { 0 });
        }
    }

    /// Sets the `last_in_chain` flag of the buffer.
    ///
    /// # Arguments
    ///
    /// * `last` - A boolean indicating whether the buffer is the last buffer in a chain of buffers.
    fn set_last_in_chain(&mut self, last: bool) {
        let buf = self.as_ngx_buf_mut();
        unsafe {
            (*buf).set_last_in_chain(if last { 1 } else { 0 });
        }
    }
}

/// The `MutableBuffer` trait extends the `Buffer` trait and provides methods for working with a
/// mutable buffer.
pub trait MutableBuffer: Buffer {
    /// Returns a mutable reference to the buffer contents as a byte slice.
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let buf = self.as_ngx_buf_mut();
        unsafe { slice::from_raw_parts_mut((*buf).pos, self.len()) }
    }

    /// Returns how many bytes can still be appended.
    ///
    /// A buffer from [`crate::core::Pool::create_buffer`] starts empty, with
    /// its whole allocation spare.
    fn spare_capacity(&self) -> usize {
        let buf = self.as_ngx_buf();
        unsafe { usize::wrapping_sub((*buf).end as _, (*buf).last as _) }
    }

    /// Appends `bytes` to the buffer contents.
    ///
    /// Returns `None` and writes nothing if they do not fit, so a caller that
    /// ignores the result cannot end up with a truncated value.
    fn append(&mut self, bytes: &[u8]) -> Option<()> {
        if bytes.len() > self.spare_capacity() {
            return None;
        }

        let buf = self.as_ngx_buf_mut();
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), (*buf).last, bytes.len());
            (*buf).last = (*buf).last.add(bytes.len());
        }

        Some(())
    }
}

/// Wrapper struct for a temporary buffer, providing methods for working with an `ngx_buf_t`.
pub struct TemporaryBuffer(*mut ngx_buf_t);

impl TemporaryBuffer {
    /// Creates a new `TemporaryBuffer` from an `ngx_buf_t` pointer.
    ///
    /// # Panics
    /// Panics if the given buffer pointer is null.
    pub fn from_ngx_buf(buf: *mut ngx_buf_t) -> TemporaryBuffer {
        assert!(!buf.is_null());
        TemporaryBuffer(buf)
    }
}

impl Buffer for TemporaryBuffer {
    /// Returns the underlying `ngx_buf_t` pointer as a raw pointer.
    fn as_ngx_buf(&self) -> *const ngx_buf_t {
        self.0
    }

    /// Returns a mutable reference to the underlying `ngx_buf_t` pointer.
    fn as_ngx_buf_mut(&mut self) -> *mut ngx_buf_t {
        self.0
    }
}

impl MutableBuffer for TemporaryBuffer {
    /// Returns a mutable reference to the buffer contents as a byte slice.
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut((*self.0).pos, self.len()) }
    }
}

/// Wrapper struct for a memory buffer, providing methods for working with an `ngx_buf_t`.
pub struct MemoryBuffer(*mut ngx_buf_t);

impl MemoryBuffer {
    /// Creates a new `MemoryBuffer` from an `ngx_buf_t` pointer.
    ///
    /// # Panics
    /// Panics if the given buffer pointer is null.
    pub fn from_ngx_buf(buf: *mut ngx_buf_t) -> MemoryBuffer {
        assert!(!buf.is_null());
        MemoryBuffer(buf)
    }
}

impl Buffer for MemoryBuffer {
    /// Returns the underlying `ngx_buf_t` pointer as a raw pointer.
    fn as_ngx_buf(&self) -> *const ngx_buf_t {
        self.0
    }

    /// Returns a mutable reference to the underlying `ngx_buf_t` pointer.
    fn as_ngx_buf_mut(&mut self) -> *mut ngx_buf_t {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the shape `ngx_create_temp_buf` produces: `pos == last == start`.
    fn temp_buf(storage: &mut [u8], raw: &mut ngx_buf_t) -> TemporaryBuffer {
        raw.start = storage.as_mut_ptr();
        raw.pos = raw.start;
        raw.last = raw.start;
        raw.end = unsafe { raw.start.add(storage.len()) };
        TemporaryBuffer::from_ngx_buf(raw)
    }

    #[test]
    fn append_fills_a_freshly_created_buffer() {
        let mut storage = [0u8; 64];
        let mut raw: ngx_buf_t = unsafe { core::mem::zeroed() };
        let mut buf = temp_buf(&mut storage, &mut raw);

        // A new buffer holds nothing, and all of its allocation is spare.
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.spare_capacity(), 64);
        assert!(buf.as_bytes_mut().is_empty());

        assert!(buf.append(b"hello ").is_some());
        assert!(buf.append(b"world").is_some());

        assert_eq!(buf.as_bytes(), b"hello world");
        assert_eq!(buf.len(), 11);
        assert_eq!(buf.spare_capacity(), 53);
    }

    #[test]
    fn append_writes_nothing_when_it_does_not_fit() {
        let mut storage = [0u8; 4];
        let mut raw: ngx_buf_t = unsafe { core::mem::zeroed() };
        let mut buf = temp_buf(&mut storage, &mut raw);

        assert!(buf.append(b"too long").is_none());
        assert_eq!(buf.len(), 0);

        // A partial append would have left the buffer holding "too ".
        assert!(buf.append(b"ok").is_some());
        assert_eq!(buf.as_bytes(), b"ok");
    }
}
