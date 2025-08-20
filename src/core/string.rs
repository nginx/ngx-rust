use core::cmp;
use core::fmt;
use core::str::{self, Utf8Error};

#[cfg(feature = "alloc")]
use alloc::{borrow::Cow, string::String};

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

#[cfg(feature = "alloc")]
pub use self::_alloc::NgxString;

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
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
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
mod _alloc {
    use core::borrow::Borrow;
    use core::hash;
    use core::ops;
    use core::ptr;

    use super::*;

    use crate::allocator::{self, Allocator};
    use crate::collections::{TryReserveError, Vec};

    /// Owned byte string type with Allocator support.
    ///
    /// Inspired by [bstr] and unstable [feature(bstr)], with two important differences:
    ///  - Allocator always have to be specified,
    ///  - any allocating methods are failible and require explicit handling of the result.
    ///
    ///  [bstr]: https://docs.rs/bstr/latest/bstr/
    ///  [feature(bstr)]: https://github.com/rust-lang/rust/issues/134915
    #[derive(Clone)]
    #[repr(transparent)]
    pub struct NgxString<A>(Vec<u8, A>)
    where
        A: Allocator + Clone;

    impl<A> NgxString<A>
    where
        A: Allocator + Clone,
    {
        /// Constructs a new, empty `NgxString<A>`.
        ///
        /// No allocations will be made until data is added to the string.
        pub fn new_in(alloc: A) -> Self {
            Self(Vec::new_in(alloc))
        }

        /// Tries to construct a new `NgxString<A>` from a byte slice.
        #[inline]
        pub fn try_from_bytes_in(
            bytes: impl AsRef<[u8]>,
            alloc: A,
        ) -> Result<Self, TryReserveError> {
            let mut this = Self::new_in(alloc);
            this.try_reserve_exact(bytes.as_ref().len())?;
            this.0.extend_from_slice(bytes.as_ref());
            Ok(this)
        }

        /// Returns a reference to the underlying allocator
        #[inline]
        pub fn allocator(&self) -> &A {
            self.0.allocator()
        }

        /// Returns this `NgxString`'s capacity, in bytes.
        #[inline]
        pub fn capacity(&self) -> usize {
            self.0.capacity()
        }

        /// Returns `true` if this `NgxString` has a length of zero, and `false` otherwise.
        #[inline]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        /// Return this `NgxString`'s length, in bytes.
        #[inline]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Appends bytes if there is sufficient spare capacity.
        ///
        /// Returns the number of remaining bytes on overflow.
        #[inline]
        pub fn append_within_capacity(&mut self, other: impl AsRef<[u8]>) -> Result<(), usize> {
            let other = other.as_ref();
            if self.0.len() == self.0.capacity() {
                return Err(other.len());
            }

            let n = cmp::min(self.0.capacity() - self.0.len(), other.len());
            unsafe {
                // SAFETY:
                //  - self.0 has at least n writable bytes allocated past self.0.len(),
                //  - other has at least n bytes available for reading.
                //  - self.0 internal buffer will be initialized until len + n after this operation
                //  - other is not borrowed from `self`
                let p = self.0.as_mut_ptr().add(self.0.len());
                ptr::copy_nonoverlapping(other.as_ptr(), p, n);
                self.0.set_len(self.0.len() + n);
            }

            match other.len() - n {
                0 => Ok(()),
                x => Err(x),
            }
        }

        /// Tries to append the bytes to the `NgxString`.
        #[inline]
        pub fn try_append(&mut self, other: impl AsRef<[u8]>) -> Result<(), TryReserveError> {
            let other = other.as_ref();
            self.0.try_reserve_exact(other.len())?;
            self.0.extend_from_slice(other);
            Ok(())
        }

        /// Tries to reserve capacity for at least `additional` more bytes.
        #[inline]
        pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
            self.0.try_reserve(additional)
        }

        /// Tries to reserve the minimum capacity for at least `additional` more bytes.
        #[inline]
        pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError> {
            self.0.try_reserve_exact(additional)
        }

        #[inline]
        pub(crate) fn as_bytes(&self) -> &[u8] {
            &self.0
        }

        #[inline]
        pub(crate) fn as_bytes_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }

        #[inline]
        pub(crate) fn as_ngx_str(&self) -> &NgxStr {
            NgxStr::from_bytes(self.0.as_slice())
        }

        #[inline]
        pub(crate) fn as_ngx_str_mut(&mut self) -> &mut NgxStr {
            NgxStr::from_bytes_mut(self.0.as_mut_slice())
        }

        /// Creates NgxString directly from a pointer, a capacity, a length and an allocator.
        ///
        /// # Safety
        ///
        /// See [Vec::from_raw_parts_in]
        #[inline]
        pub unsafe fn from_raw_parts(
            ptr: *mut u8,
            length: usize,
            capacity: usize,
            alloc: A,
        ) -> Self {
            Self(Vec::from_raw_parts_in(ptr, length, capacity, alloc))
        }

        /// Splits the NgxString into its raw components.
        ///
        /// The caller becomes responsible for the memory previously managed by this NgxString.
        #[inline]
        pub fn into_raw_parts(self) -> (*mut u8, usize, usize, A) {
            self.0.into_raw_parts_with_alloc()
        }
    }

    impl<A> AsRef<NgxStr> for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn as_ref(&self) -> &NgxStr {
            self.as_ngx_str()
        }
    }

    impl<A> AsMut<NgxStr> for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn as_mut(&mut self) -> &mut NgxStr {
            self.as_ngx_str_mut()
        }
    }

    impl<A> AsRef<[u8]> for NgxString<A>
    where
        A: Allocator + Clone,
    {
        #[inline]
        fn as_ref(&self) -> &[u8] {
            self.as_bytes()
        }
    }

    impl<A> AsMut<[u8]> for NgxString<A>
    where
        A: Allocator + Clone,
    {
        #[inline]
        fn as_mut(&mut self) -> &mut [u8] {
            self.as_bytes_mut()
        }
    }

    impl<A> Borrow<NgxStr> for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn borrow(&self) -> &NgxStr {
            self.as_ngx_str()
        }
    }

    impl<A> Borrow<[u8]> for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn borrow(&self) -> &[u8] {
            self.0.as_slice()
        }
    }

    impl<A> fmt::Debug for NgxString<A>
    where
        A: Allocator + Clone,
    {
        #[inline]
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            // XXX: Use debug_tuple() and feature(debug_closure_helpers) once it's stabilized
            f.write_str("NgxString(")?;
            nginx_sys::detail::debug_bytes(f, &self.0)?;
            f.write_str(")")
        }
    }

    impl<A> ops::Deref for NgxString<A>
    where
        A: Allocator + Clone,
    {
        type Target = NgxStr;

        fn deref(&self) -> &Self::Target {
            self.as_ngx_str()
        }
    }

    impl<A> ops::DerefMut for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.as_ngx_str_mut()
        }
    }

    impl<A> fmt::Display for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(self.as_ngx_str(), f)
        }
    }

    impl<A> hash::Hash for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn hash<H: hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state);
        }
    }

    // `NgxString`'s with different allocators should be comparable

    impl<A1, A2> PartialEq<NgxString<A2>> for NgxString<A1>
    where
        A1: Allocator + Clone,
        A2: Allocator + Clone,
    {
        fn eq(&self, other: &NgxString<A2>) -> bool {
            PartialEq::eq(self.as_bytes(), other.as_bytes())
        }
    }

    impl<A> Eq for NgxString<A> where A: Allocator + Clone {}

    impl<A1, A2> PartialOrd<NgxString<A2>> for NgxString<A1>
    where
        A1: Allocator + Clone,
        A2: Allocator + Clone,
    {
        fn partial_cmp(&self, other: &NgxString<A2>) -> Option<cmp::Ordering> {
            Some(Ord::cmp(self.as_bytes(), other.as_bytes()))
        }
    }

    impl<A> Ord for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn cmp(&self, other: &Self) -> cmp::Ordering {
            Ord::cmp(self.as_bytes(), other.as_bytes())
        }
    }

    impl<OA: Allocator + Clone> allocator::TryCloneIn for NgxString<OA> {
        type Target<A: Allocator + Clone> = NgxString<A>;

        fn try_clone_in<A: Allocator + Clone>(
            &self,
            alloc: A,
        ) -> Result<Self::Target<A>, allocator::AllocError> {
            NgxString::try_from_bytes_in(self.as_bytes(), alloc).map_err(|_| allocator::AllocError)
        }
    }

    impl<A> fmt::Write for NgxString<A>
    where
        A: Allocator + Clone,
    {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            self.append_within_capacity(s).map_err(|_| fmt::Error)
        }
    }

    // Implement byte comparisons directly, leave the rest to Deref<Target = NgxStr>.

    impl_partial_eq!(NgxString<A>, &'a [u8]; A: Allocator + Clone);
    impl_partial_eq!(NgxString<A>, &'a [u8; N]; A: Allocator + Clone, const N: usize);
    impl_partial_eq!(NgxString<A>, &'a NgxStr; A: Allocator + Clone);
    impl_partial_eq!(NgxString<A>, ngx_str_t; A: Allocator + Clone);

    impl_partial_ord!(NgxString<A>, &'a [u8]; A: Allocator + Clone);
    impl_partial_ord!(NgxString<A>, &'a [u8; N]; A: Allocator + Clone, const N: usize);
    impl_partial_ord!(NgxString<A>, &'a NgxStr; A: Allocator + Clone);
    impl_partial_ord!(NgxString<A>, ngx_str_t; A: Allocator + Clone);

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
    fn test_str_comparisons() {
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
    #[cfg(feature = "alloc")]
    fn test_string_comparisons() {
        use crate::allocator::Global;

        let string = "test".to_string();
        let ngx_string = ngx_str_t {
            data: string.as_ptr().cast_mut(),
            len: string.len(),
        };
        let borrowed: &NgxStr = string.as_bytes().into();
        let owned = NgxString::try_from_bytes_in(&string, Global).unwrap();

        assert_eq!(string.as_bytes(), owned);
        assert_eq!(ngx_string, owned);
        assert_eq!(borrowed, owned);
        assert_eq!(b"test", owned);
        assert_eq!(owned, string.as_bytes());
        assert_eq!(owned, ngx_string);
        assert_eq!(owned, borrowed);
        assert_eq!(owned, b"test");

        // String comparisons via Deref<Target = NgxStr>
        assert_eq!(string, *owned);
        assert_eq!(string.as_str(), *owned);
        assert_eq!("test", *owned);
        assert_eq!(*owned, string);
        assert_eq!(*owned, string.as_str());
        assert_eq!(*owned, "test");
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn test_string_write() {
        use core::fmt::Write;

        use crate::allocator::Global;

        let h = NgxStr::from_bytes(b"Hello");
        let w = NgxStr::from_bytes(b"world");

        let mut s = NgxString::new_in(Global);
        s.try_reserve(16).expect("reserve");

        // Remember ptr and len of internal buffer
        let saved = (s.as_bytes().as_ptr(), s.capacity());

        write!(s, "{h} {w}!").expect("write");

        assert_eq!(s, b"Hello world!");
        assert_eq!((s.as_bytes().as_ptr(), s.capacity()), saved);
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
