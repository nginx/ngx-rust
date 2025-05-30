//! Implementation details shared between nginx-sys and ngx.
#![allow(missing_docs)]

use core::fmt;
use core::ptr::copy_nonoverlapping;

use crate::bindings::{ngx_pnalloc, ngx_pool_t, u_char};

/// Convert a byte slice to a raw pointer (`*mut u_char`) allocated in the given nginx memory pool.
///
/// # Safety
///
/// The caller must provide a valid pointer to the memory pool.
pub unsafe fn bytes_to_uchar(pool: *mut ngx_pool_t, data: &[u8]) -> Option<*mut u_char> {
    let ptr: *mut u_char = ngx_pnalloc(pool, data.len()) as _;
    if ptr.is_null() {
        return None;
    }
    copy_nonoverlapping(data.as_ptr(), ptr, data.len());
    Some(ptr)
}

/// Convert a string slice (`&str`) to a raw pointer (`*mut u_char`) allocated in the given nginx
/// memory pool.
///
/// # Arguments
///
/// * `pool` - A pointer to the nginx memory pool (`ngx_pool_t`).
/// * `data` - The string slice to convert to a raw pointer.
///
/// # Safety
/// This function is marked as unsafe because it involves raw pointer manipulation and direct memory
/// allocation using `ngx_pnalloc`.
///
/// # Returns
/// A raw pointer (`*mut u_char`) to the allocated memory containing the converted string data.
///
/// # Example
/// ```rust,ignore
/// let pool: *mut ngx_pool_t = ...; // Obtain a pointer to the nginx memory pool
/// let data: &str = "example"; // The string to convert
/// let ptr = str_to_uchar(pool, data);
/// ```
pub unsafe fn str_to_uchar(pool: *mut ngx_pool_t, data: &str) -> *mut u_char {
    let ptr: *mut u_char = ngx_pnalloc(pool, data.len()) as _;
    debug_assert!(!ptr.is_null());
    copy_nonoverlapping(data.as_ptr(), ptr, data.len());
    ptr
}

#[inline]
pub fn debug_bytes(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    if f.alternate() {
        match bytes.len() {
            0 => Ok(()),
            1 => write!(f, "{:02x}", bytes[0]),
            x => {
                for b in &bytes[..x - 1] {
                    write!(f, "{b:02x},")?;
                }
                write!(f, "{:02x}", bytes[x - 1])
            }
        }
    } else {
        f.write_str("\"")?;
        display_bytes(f, bytes)?;
        f.write_str("\"")
    }
}

#[inline]
pub fn display_bytes(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    // The implementation is similar to an inlined `String::from_utf8_lossy`, with two
    // important differences:
    //
    //  - it writes directly to the Formatter instead of allocating a temporary String
    //  - invalid sequences are represented as escaped individual bytes
    for chunk in bytes.utf8_chunks() {
        f.write_str(chunk.valid())?;
        for byte in chunk.invalid() {
            write!(f, "\\x{byte:02x}")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::format;
    use alloc::string::ToString;

    use super::*;

    struct TestStr(&'static [u8]);

    impl fmt::Debug for TestStr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("TestStr(")?;
            debug_bytes(f, self.0)?;
            f.write_str(")")
        }
    }

    impl fmt::Display for TestStr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            display_bytes(f, self.0)
        }
    }

    #[test]
    fn test_display() {
        let cases: &[(&[u8], &str)] = &[
            (b"", ""),
            (b"Ferris the \xf0\x9f\xa6\x80", "Ferris the 🦀"),
            (b"\xF0\x90\x80", "\\xf0\\x90\\x80"),
            (b"\xF0\x90\x80Hello World", "\\xf0\\x90\\x80Hello World"),
            (b"Hello \xF0\x90\x80World", "Hello \\xf0\\x90\\x80World"),
            (b"Hello World\xF0\x90\x80", "Hello World\\xf0\\x90\\x80"),
        ];

        for (bytes, expected) in cases {
            let str = TestStr(bytes);
            assert_eq!(str.to_string(), *expected);
        }

        // Check that the formatter arguments are ignored correctly
        for (bytes, expected) in &cases[2..3] {
            let str = TestStr(bytes);
            assert_eq!(format!("{str:12.12}"), *expected);
        }
    }

    #[test]
    fn test_debug() {
        let cases: &[(&[u8], &str, &str)] = &[
            (b"", "TestStr(\"\")", "TestStr()"),
            (b"a", "TestStr(\"a\")", "TestStr(61)"),
            (
                b"Ferris the \xf0\x9f\xa6\x80",
                "TestStr(\"Ferris the 🦀\")",
                "TestStr(46,65,72,72,69,73,20,74,68,65,20,f0,9f,a6,80)",
            ),
            (
                b"\xF0\x90\x80",
                "TestStr(\"\\xf0\\x90\\x80\")",
                "TestStr(f0,90,80)",
            ),
        ];
        for (bytes, expected, alternate) in cases {
            let str = TestStr(bytes);
            assert_eq!(format!("{str:?}"), *expected);
            assert_eq!(format!("{str:#?}"), *alternate);
        }
    }
}
