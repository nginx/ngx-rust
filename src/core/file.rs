//! File, directory and temporary file operations.

use core::ffi::CStr;
use core::marker::PhantomData;
use core::mem;
use core::ptr::{self, NonNull};

use errno::Errno;
use nginx_sys::{
    ngx_dir_t, ngx_fd_t, ngx_file_info_t, ngx_file_t, ngx_file_uniq_t, ngx_log_t, ngx_str_t,
    ngx_uint_t, off_t, time_t,
};

/// Provides access to an open file on the filesystem.
#[repr(transparent)]
pub struct File<'name>(ngx_file_t, PhantomData<&'name CStr>);

impl<'name> File<'name> {
    /// Opens or creates a new file.
    pub fn open(
        log: NonNull<ngx_log_t>,
        name: &'name CStr,
        mode: ngx_uint_t,
        create: ngx_uint_t,
        access: ngx_uint_t,
    ) -> Result<Self, Errno> {
        // Lifetime preserved as 'name
        let name = ngx_str_t { data: name.as_ptr().cast_mut().cast(), len: name.count_bytes() };

        let fd =
            unsafe { nginx_sys::ngx_open_file(name.data, mode as _, create as _, access as _) };
        if fd == nginx_sys::NGX_INVALID_FILE {
            return Err(errno::errno());
        }

        let mut this = Self(
            ngx_file_t { fd, name, log: log.as_ptr(), ..unsafe { mem::zeroed() } },
            PhantomData,
        );

        ngx_try_file(unsafe { nginx_sys::ngx_fd_info(fd, &raw mut this.0.info) })?;

        if mode & nginx_sys::NGX_FILE_APPEND == nginx_sys::NGX_FILE_APPEND {
            this.0.offset = this.info().size();
        }

        Ok(this)
    }

    /// Returns information about the file.
    pub fn info(&self) -> &FileInfo {
        // SAFETY: FileInfo is a transparent wrapper over ngx_file_info_t
        unsafe { mem::transmute(&self.0.info) }
    }

    /// Reads from the file into the specified buffer, returning how many bytes were read.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        let off = self.0.offset;

        match unsafe { nginx_sys::ngx_read_file(&raw mut self.0, buf.as_mut_ptr(), buf.len(), off) }
        {
            nginx_sys::NGX_ERROR => Err(errno::errno()),
            x => Ok(x as usize),
        }
    }

    /// Writes a buffer into the file, returning how many bytes were written.
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, Errno> {
        let off = self.0.offset;

        match unsafe {
            nginx_sys::ngx_write_file(&raw mut self.0, buf.as_ptr().cast_mut(), buf.len(), off)
        } {
            nginx_sys::NGX_ERROR => Err(errno::errno()),
            x => Ok(x as usize),
        }
    }

    /// Writes a chain of buffers into the file, returning how many bytes were written.
    pub fn write_chain(&mut self, cl: &nginx_sys::ngx_chain_t) -> Result<usize, Errno> {
        let offset = self.0.offset;

        match unsafe {
            nginx_sys::ngx_write_chain_to_file(
                &raw mut self.0,
                ptr::from_ref(cl).cast_mut(), // not modified
                offset,
                ptr::null_mut(), // unused since release-1.9.13
            )
        } {
            nginx_sys::NGX_ERROR => Err(errno::errno()),
            x => Ok(x as usize),
        }
    }
}

#[cfg(all(feature = "std", unix))]
impl std::os::fd::AsFd for File<'_> {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        // SAFETY: File instance should always have a valid fd
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.0.fd) }
    }
}

#[cfg(all(feature = "std", windows))]
impl std::os::windows::io::AsHandle for File<'_> {
    fn as_handle(&self) -> std::os::windows::io::BorrowedHandle<'_> {
        // SAFETY: File instance should always have a valid handle
        unsafe { std::os::windows::io::BorrowedHandle::borrow_raw(self.0.fd) }
    }
}

#[cfg(feature = "std")]
impl std::io::Read for File<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read(buf).map_err(Into::into)
    }
}

#[cfg(feature = "std")]
impl std::io::Write for File<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write(buf).map_err(Into::into)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(()) // fsyncdata?
    }
}

impl Drop for File<'_> {
    fn drop(&mut self) {
        if self.0.fd == nginx_sys::NGX_INVALID_FILE {
            return;
        }

        if unsafe { nginx_sys::ngx_close_file(self.0.fd) } == nginx_sys::NGX_FILE_ERROR as _ {
            crate::ngx_log_error!(
                nginx_sys::NGX_LOG_ALERT,
                self.0.log,
                "{:?} \"{:?}\" failed: {}",
                nginx_sys::ngx_close_file_n,
                self.0.name,
                errno::errno()
            );
        }
    }
}

/// Describes information about a file.
#[repr(transparent)]
pub struct FileInfo(ngx_file_info_t);

impl FileInfo {
    /// Returns information about a file.
    pub fn new_for_file(file: &CStr) -> Result<Self, Errno> {
        let mut fi: ngx_file_info_t = unsafe { mem::zeroed() };

        ngx_try_file(unsafe {
            nginx_sys::ngx_file_info(file.as_ptr().cast_mut().cast(), &raw mut fi)
        })?;
        Ok(Self(fi))
    }

    /// Returns information about a file descriptor.
    // HANDLE is not a real pointer and will not be dereferenced.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn new_for_fd(fd: ngx_fd_t) -> Result<Self, Errno> {
        let mut fi: ngx_file_info_t = unsafe { mem::zeroed() };

        ngx_try_file(unsafe { nginx_sys::ngx_fd_info(fd, &raw mut fi) })?;
        Ok(Self(fi))
    }

    /// Returns information about a symbolic link.
    pub fn new_for_link(file: &CStr) -> Result<Self, Errno> {
        let mut fi: ngx_file_info_t = unsafe { mem::zeroed() };

        ngx_try_file(unsafe {
            nginx_sys::ngx_link_info(file.as_ptr().cast_mut().cast(), &raw mut fi)
        })?;
        Ok(Self(fi))
    }

    /// Returns `true` if this [FileInfo] is for directory.
    pub fn is_dir(&self) -> bool {
        unsafe { nginx_sys::ngx_is_dir(&raw const self.0) }
    }

    /// Returns `true` if this [FileInfo] is for regular file.
    pub fn is_file(&self) -> bool {
        unsafe { nginx_sys::ngx_is_file(&raw const self.0) }
    }

    /// Returns `true` if this [FileInfo] is for symbolic link.
    pub fn is_link(&self) -> bool {
        unsafe { nginx_sys::ngx_is_link(&raw const self.0) }
    }

    /// Returns `true` if this [FileInfo] is for executable file.
    pub fn is_exec(&self) -> bool {
        unsafe { nginx_sys::ngx_is_exec(&raw const self.0) }
    }

    /// Returns file access mode.
    pub fn access(&self) -> ngx_uint_t {
        unsafe { nginx_sys::ngx_file_access(&raw const self.0) as _ }
    }

    /// Returns the size of the file in bytes.
    pub fn size(&self) -> off_t {
        unsafe { nginx_sys::ngx_file_size(&raw const self.0) }
    }

    /// Returns the size of the file, rounded up to filesystem blocks.
    pub fn fs_size(&self) -> off_t {
        unsafe { nginx_sys::ngx_file_fs_size(&raw const self.0) }
    }

    /// Returns the last modification time of the file.
    pub fn mtime(&self) -> time_t {
        unsafe { nginx_sys::ngx_file_mtime(&raw const self.0) }
    }

    /// Returns unique file identifier.
    pub fn uniq(&self) -> ngx_file_uniq_t {
        unsafe { nginx_sys::ngx_file_uniq(&raw const self.0) }
    }
}

/// A directory iterator.
///
/// The ngx_dir_t implementation details prevent us from making a proper iterator of this type.
/// For now, the recommended usage is as follows
/// ```rust,ignore
/// use ngx::core::file::ReadDir;
///
/// let mut dir = ReadDir::new(c"/tmp")?;
///
/// while let Some(ent) = dir.next_entry()? {
///     println!("entry = {:?}", ent.name());
/// }
///
/// ```
/// Due to the implementation details of ngx_dir_t, it is not possible to make a proper iterator of
/// this type.
pub struct ReadDir<'a>(nginx_sys::ngx_dir_t, #[allow(unused)] &'a CStr);

impl<'a> ReadDir<'a> {
    /// Opens a directory for iteration.
    pub fn new(name: &'a CStr) -> Result<Self, Errno> {
        let mut de: ngx_dir_t = unsafe { mem::zeroed() };
        let mut sname =
            ngx_str_t { data: name.as_ptr().cast_mut().cast(), len: name.count_bytes() };

        ngx_try_ok(unsafe { nginx_sys::ngx_open_dir(&raw mut sname, &raw mut de) })?;
        Ok(ReadDir(de, name))
    }

    /// Reads the next directory entry.
    pub fn next_entry(&mut self) -> Result<Option<&DirEntry>, Errno> {
        #[allow(clippy::never_loop)] // Only on Windows.
        loop {
            errno::set_errno(Errno(0));

            match ngx_try_ok(unsafe { nginx_sys::ngx_read_dir(&raw mut self.0) }) {
                Err(err) if err.0 == nginx_sys::NGX_ENOMOREFILES as _ => return Ok(None),
                Err(err) => return Err(err),
                _ => (),
            };

            #[cfg(ngx_feature = "have_openat")]
            if self.0.valid_info() == 0 {
                use nginx_sys::ngx_file_at_info;

                let name = unsafe { nginx_sys::ngx_de_name(&raw mut self.0) };
                let dirfd = unsafe { libc::dirfd(self.0.dir.cast()) };

                self.0.set_type(0);

                match ngx_try_file(unsafe {
                    ngx_file_at_info(dirfd, name, &raw mut self.0.info, 0)
                }) {
                    Err(Errno(nginx_sys::NGX_ENOENT) | Errno(nginx_sys::NGX_ELOOP)) => (),
                    // Return to the caller as Ok((name, error))?
                    Err(Errno(nginx_sys::NGX_EACCES)) => continue,
                    Err(err) => return Err(err),
                    _ => break,
                }

                ngx_try_file(unsafe {
                    ngx_file_at_info(dirfd, name, &raw mut self.0.info, libc::AT_SYMLINK_NOFOLLOW)
                })?;
            }

            // finddata is always prefetched on Windows
            #[cfg(not(any(ngx_feature = "have_openat", windows)))]
            if self.0.valid_info() == 0 {
                let name = unsafe {
                    core::slice::from_raw_parts(
                        nginx_sys::ngx_de_name(&raw mut self.0),
                        nginx_sys::ngx_de_namelen(&raw mut self.0),
                    )
                };
                let mut full_path = crate::collections::Vec::<u8>::new();
                full_path
                    .try_reserve_exact(self.1.count_bytes() + 1 + name.len() + 1)
                    .map_err(|_| Errno(nginx_sys::NGX_ENOMEM))?;

                full_path.extend(self.1.to_bytes());
                full_path.push(b'/');
                full_path.extend(name);
                full_path.push(b'\0');

                match ngx_try_file(unsafe {
                    nginx_sys::ngx_de_info(full_path.as_ptr(), &raw mut self.0)
                }) {
                    Err(Errno(nginx_sys::NGX_ENOENT) | Errno(nginx_sys::NGX_ELOOP)) => (),
                    // Return to the caller as Ok((name, error))?
                    Err(Errno(nginx_sys::NGX_EACCES)) => continue,
                    Err(err) => return Err(err),
                    _ => break,
                }

                ngx_try_file(unsafe {
                    nginx_sys::ngx_de_link_info(full_path.as_ptr(), &raw mut self.0)
                })?;
            }

            break;
        }

        // SAFETY: DirEntry is a transparent wrapper over ngx_dir_t
        Ok(Some(unsafe { mem::transmute::<&ngx_dir_t, &DirEntry>(&self.0) }))
    }
}

impl Drop for ReadDir<'_> {
    fn drop(&mut self) {
        unsafe { nginx_sys::ngx_close_dir(&raw mut self.0) };
    }
}

/// Entries returned by [ReadDir].
#[repr(transparent)]
pub struct DirEntry(ngx_dir_t);

impl DirEntry {
    /// Returns directory entry name.
    pub fn name(&self) -> &[u8] {
        unsafe {
            let data = nginx_sys::ngx_de_name(&raw const self.0);
            let len = nginx_sys::ngx_de_namelen(&raw const self.0);
            core::slice::from_raw_parts(data, len)
        }
    }

    /// Returns `true` if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        unsafe { nginx_sys::ngx_de_is_dir(&raw const self.0) }
    }

    /// Returns `true` if this entry is a regular file.
    pub fn is_file(&self) -> bool {
        unsafe { nginx_sys::ngx_de_is_file(&raw const self.0) }
    }

    /// Returns `true` if this entry is a symbolic link.
    pub fn is_link(&self) -> bool {
        unsafe { nginx_sys::ngx_de_is_link(&raw const self.0) }
    }

    /// Returns directory entry access mode.
    pub fn access(&self) -> ngx_uint_t {
        unsafe { nginx_sys::ngx_de_access(&raw const self.0) }
    }

    /// Returns the size of the directory entry in bytes.
    pub fn size(&self) -> off_t {
        unsafe { nginx_sys::ngx_de_size(&raw const self.0) }
    }

    /// Returns the size of the entry, rounded up to filesystem blocks.
    pub fn fs_size(&self) -> off_t {
        unsafe { nginx_sys::ngx_de_fs_size(&raw const self.0) }
    }

    /// Returns the last modification time of the entry.
    pub fn mtime(&self) -> time_t {
        unsafe { nginx_sys::ngx_de_mtime(&raw const self.0) }
    }
}

fn ngx_try_file<T>(s: T) -> Result<T, Errno>
where
    T: Copy + Into<nginx_sys::ngx_int_t>,
{
    if s.into() == nginx_sys::NGX_FILE_ERROR { Err(errno::errno()) } else { Ok(s) }
}

fn ngx_try_ok<T>(s: T) -> Result<(), Errno>
where
    T: Copy + Into<nginx_sys::ngx_int_t>,
{
    if s.into() != nginx_sys::NGX_OK { Err(errno::errno()) } else { Ok(()) }
}
