#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::{borrow::Cow, string::String};
use core::cmp;
use core::fmt;
use core::str::{self, Utf8Error};
#[cfg(feature = "std")]
use std::{borrow::Cow, string::String};

use crate::ffi::{ngx_str_t, u_char};

/// Static string initializer for [`ngx_str_t`].
///
/// The resulting byte string is always nul-terminated (just like a C string).
///
/// [`ngx_str_t`]: https://nginx.org/en/docs/dev/development_guide.html#string_overview
#[macro_export]
macro_rules! ngx_string {
    ($s:expr) => {{
        $crate::ffi::ngx_str_t {
            len: $s.len() as _,
            data: concat!($s, "\0").as_ptr() as *mut u8,
        }
    }};
}

/// Representation of a borrowed [Nginx string].
///
/// [Nginx string]: https://nginx.org/en/docs/dev/development_guide.html#string_overview
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct NgxStr([u_char]);

impl NgxStr {
    /// Create an [`NgxStr`] from an [`ngx_str_t`].
    ///
    /// [`ngx_str_t`]: https://nginx.org/en/docs/dev/development_guide.html#string_overview
    ///
    /// # Safety
    ///
    /// The caller has provided a valid `ngx_str_t` with a `data` pointer that points
    /// to range of bytes of at least `len` bytes, whose content remains valid and doesn't
    /// change for the lifetime of the returned `NgxStr`.
    pub unsafe fn from_ngx_str<'a>(str: ngx_str_t) -> &'a NgxStr {
        let bytes: &[u8] = str.as_bytes();
        &*(bytes as *const [u8] as *const NgxStr)
    }

    /// Create an [NgxStr] from a borrowed byte slice.
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> &Self {
        // SAFETY: An `NgxStr` is identical to a `[u8]` slice, given `u_char` is an alias for `u8`
        unsafe { &*(bytes as *const [u8] as *const NgxStr) }
    }

    /// Create a mutable [NgxStr] from a borrowed byte slice.
    #[inline]
    pub fn from_bytes_mut(bytes: &mut [u8]) -> &mut Self {
        // SAFETY: An `NgxStr` is identical to a `[u8]` slice, given `u_char` is an alias for `u8`
        unsafe { &mut *(bytes as *mut [u8] as *mut NgxStr) }
    }

    /// Access the [`NgxStr`] as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Yields a `&str` slice if the [`NgxStr`] contains valid UTF-8.
    pub fn to_str(&self) -> Result<&str, Utf8Error> {
        str::from_utf8(self.as_bytes())
    }

    /// Converts an [`NgxStr`] into a [`Cow<str>`], replacing invalid UTF-8 sequences.
    ///
    /// See [`String::from_utf8_lossy`].
    #[cfg(feature = "alloc")]
    pub fn to_string_lossy(&self) -> Cow<str> {
        String::from_utf8_lossy(self.as_bytes())
    }

    /// Returns `true` if the [`NgxStr`] is empty, otherwise `false`.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<[u8]> for NgxStr {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsMut<[u8]> for NgxStr {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl fmt::Debug for NgxStr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // XXX: Use debug_tuple() and feature(debug_closure_helpers) once it's stabilized
        f.write_str("NgxStr(")?;
        nginx_sys::detail::debug_bytes(f, &self.0)?;
        f.write_str(")")
    }
}

impl Default for &NgxStr {
    fn default() -> Self {
        NgxStr::from_bytes(&[])
    }
}

impl fmt::Display for NgxStr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        nginx_sys::detail::display_bytes(f, &self.0)
    }
}

macro_rules! impl_partial_ord_eq_from {
    ($self:ty, $other:ty) => { impl_partial_ord_eq_from!($self, $other;); };

    ($self:ty, $other:ty; $($args:tt)*) => {
        impl<'a, $($args)*> From<$other> for &'a NgxStr {
            #[inline]
            fn from(other: $other) -> Self {
                let other: &[u8] = other.as_ref();
                NgxStr::from_bytes(other)
            }
        }

        impl_partial_eq!($self, $other; $($args)*);
        impl_partial_ord!($self, $other; $($args)*);
    };
}

macro_rules! impl_partial_eq {
    ($self:ty, $other:ty) => { impl_partial_eq!($self, $other;); };

    ($self:ty, $other:ty; $($args:tt)*) => {
        impl<'a, $($args)*> PartialEq<$other> for $self {
            #[inline]
            fn eq(&self, other: &$other) -> bool {
                let other: &[u8] = other.as_ref();
                PartialEq::eq(self.as_bytes(), other)
            }
        }

        impl<'a, $($args)*> PartialEq<$self> for $other {
            #[inline]
            fn eq(&self, other: &$self) -> bool {
                let this: &[u8] = self.as_ref();
                PartialEq::eq(this, other.as_bytes())
            }
        }
    };
}

macro_rules! impl_partial_ord {
    ($self:ty, $other:ty) => { impl_partial_ord!($self, $other;); };

    ($self:ty, $other:ty; $($args:tt)*) => {
       impl<'a, $($args)*> PartialOrd<$other> for $self {
            #[inline]
            fn partial_cmp(&self, other: &$other) -> Option<cmp::Ordering> {
                let other: &[u8] = other.as_ref();
                PartialOrd::partial_cmp(self.as_bytes(), other)
            }
        }

        impl<'a, $($args)*> PartialOrd<$self> for $other {
            #[inline]
            fn partial_cmp(&self, other: &$self) -> Option<cmp::Ordering> {
                let this: &[u8] = self.as_ref();
                PartialOrd::partial_cmp(this, other.as_bytes())
            }
        }
    };
}

impl_partial_eq!(NgxStr, [u8]);
impl_partial_eq!(NgxStr, [u8; N]; const N: usize);
impl_partial_eq!(NgxStr, str);
impl_partial_eq!(NgxStr, ngx_str_t);
impl_partial_eq!(&'a NgxStr, ngx_str_t);
impl_partial_ord!(NgxStr, [u8]);
impl_partial_ord!(NgxStr, [u8; N]; const N: usize);
impl_partial_ord!(NgxStr, str);
impl_partial_ord!(NgxStr, ngx_str_t);
impl_partial_ord!(&'a NgxStr, ngx_str_t);
impl_partial_ord_eq_from!(NgxStr, &'a [u8]);
impl_partial_ord_eq_from!(NgxStr, &'a [u8; N]; const N: usize);
impl_partial_ord_eq_from!(NgxStr, &'a str);

#[cfg(feature = "alloc")]
mod _alloc_impls {
    use super::*;
    impl_partial_eq!(NgxStr, String);
    impl_partial_eq!(&'a NgxStr, String);
    impl_partial_ord!(NgxStr, String);
    impl_partial_ord!(&'a NgxStr, String);
    impl_partial_ord_eq_from!(NgxStr, &'a String);
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::string::ToString;

    use super::*;

    #[test]
    fn test_comparisons() {
        let string = "test".to_string();
        let ngx_string = ngx_str_t {
            data: string.as_ptr().cast_mut(),
            len: string.len(),
        };
        let ns: &NgxStr = string.as_bytes().into();

        #[cfg(feature = "alloc")]
        assert_eq!(string, ns);
        assert_eq!(ngx_string, ns);
        assert_eq!(string.as_bytes(), ns);
        assert_eq!(string.as_str(), ns);
        assert_eq!(b"test", ns);
        assert_eq!("test", ns);

        #[cfg(feature = "alloc")]
        assert_eq!(ns, string);
        assert_eq!(ns, ngx_string);
        assert_eq!(ns, string.as_bytes());
        assert_eq!(ns, string.as_str());
        assert_eq!(ns, b"test");
        assert_eq!(ns, "test");
    }

    #[test]
    fn test_lifetimes() {
        let a: &NgxStr = "Hello World!".into();

        let s = "Hello World!".to_string();
        let b: &NgxStr = s.as_bytes().into();

        // The compiler should detect that s is borrowed and fail.
        // drop(s); // ☢️

        assert_eq!(a.0, b.0);
    }
}
