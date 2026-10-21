#![allow(missing_docs, clippy::missing_safety_doc)]
use core::ffi::{c_int, c_uchar, c_uint, c_ulong};
use core::mem::size_of;

use cfg_if::cfg_if;

use crate::bindings::{
    ngx_dir_t, ngx_fd_t, ngx_file_info_t, ngx_file_uniq_t, ngx_int_t, ngx_uint_t, off_t, time_t,
};

// Check if the time_t as used in nginx headers is compatible with libc::time_t.
const _: () = assert!(
    core::mem::size_of::<libc::time_t>() == crate::bindings::NGX_TIME_T_SIZE,
    "
time_t type used in nginx is not compatible with libc::time_t.
This is likely caused by use of 32-bit time_t in libc and can be resolved with

    RUSTFLAGS='--cfg=libc_unstable_time64'
"
);

// Check if the off_t as used in nginx headers is compatible with libc::off_t.
#[allow(clippy::unnecessary_cast)]
const _: () = assert!(
    libc::off_t::MAX as i64 == crate::bindings::NGX_MAX_OFF_T_VALUE as i64,
    "off_t type used in nginx is not compatible with libc::off_t."
);

// Likely caused by time_t size mismatch.
const _: () = assert!(
    size_of::<ngx_file_info_t>() == size_of::<libc::stat>(),
    "ngx_file_info_t layout does not match libc::stat."
);

/*
 * ngx_config.h
 */

/// Returns a non cryptograhpically-secure pseudo-random integer.
#[inline]
pub fn ngx_random() -> core::ffi::c_long {
    // random() is not available in ::libc
    unsafe { crate::bindings::random() }
}

/*
 * ngx_files.h
 */

// c_ulong argument types match win32 version.
// Unfortunately, that means ngx_open_file is not usable without type casts.
#[inline]
pub unsafe fn ngx_open_file(
    name: *const c_uchar,
    mode: c_ulong,
    create: c_ulong,
    access: c_ulong,
) -> ngx_fd_t {
    unsafe { libc::open(name.cast(), (mode | create) as c_int, access as c_uint) }
}

pub use libc::close as ngx_close_file;

#[inline]
pub unsafe fn ngx_delete_file(name: *const c_uchar) -> ngx_int_t {
    unsafe { libc::unlink(name.cast()) as _ }
}

pub use libc::read as ngx_read_fd;
pub use libc::write as ngx_write_fd;
pub use ngx_write_fd as ngx_write_console;

#[inline]
pub unsafe fn ngx_rename_file(o: *const c_uchar, n: *const c_uchar) -> ngx_int_t {
    unsafe { libc::rename(o.cast(), n.cast()) as _ }
}

#[inline]
pub unsafe fn ngx_change_file_access(n: *const c_uchar, a: ngx_uint_t) -> ngx_int_t {
    unsafe { libc::chmod(n.cast(), a as libc::mode_t) as _ }
}

#[inline]
pub unsafe fn ngx_file_info(file: *const c_uchar, sb: *mut ngx_file_info_t) -> ngx_int_t {
    unsafe { libc::stat(file.cast(), sb.cast()) as _ }
}

#[inline]
pub unsafe fn ngx_fd_info(fd: ngx_fd_t, sb: *mut ngx_file_info_t) -> ngx_int_t {
    unsafe { libc::fstat(fd, sb.cast()) as _ }
}

#[inline]
pub unsafe fn ngx_link_info(file: *const c_uchar, sb: *mut ngx_file_info_t) -> ngx_int_t {
    unsafe { libc::lstat(file.cast(), sb.cast()) as _ }
}

#[inline]
pub unsafe fn ngx_is_dir(sb: *const ngx_file_info_t) -> bool {
    (unsafe { (*sb).st_mode } & libc::S_IFMT) == libc::S_IFDIR
}

#[inline]
pub unsafe fn ngx_is_file(sb: *const ngx_file_info_t) -> bool {
    (unsafe { (*sb).st_mode } & libc::S_IFMT) == libc::S_IFREG
}

#[inline]
pub unsafe fn ngx_is_link(sb: *const ngx_file_info_t) -> bool {
    (unsafe { (*sb).st_mode } & libc::S_IFMT) == libc::S_IFLNK
}

#[inline]
pub unsafe fn ngx_is_exec(sb: *const ngx_file_info_t) -> bool {
    (unsafe { (*sb).st_mode } & libc::S_IXUSR) == libc::S_IXUSR
}

#[inline]
pub unsafe fn ngx_file_access(sb: *const ngx_file_info_t) -> ngx_uint_t {
    (unsafe { (*sb).st_mode } & 0o777) as _
}

#[inline]
pub unsafe fn ngx_file_size(sb: *const ngx_file_info_t) -> off_t {
    unsafe { (*sb).st_size as _ }
}

#[inline]
pub unsafe fn ngx_file_fs_size(sb: *const ngx_file_info_t) -> off_t {
    let size = unsafe { (*sb).st_size } as off_t;
    let blocks = unsafe { (*sb).st_blocks } as off_t;
    let blksize = unsafe { (*sb).st_blksize } as off_t;

    if blocks * 512 > size && blocks * 512 < size + 8 * blksize {
        (blocks * 512) as _
    } else {
        size as _
    }
}

#[inline]
pub unsafe fn ngx_file_mtime(sb: *const ngx_file_info_t) -> time_t {
    unsafe { (*sb).st_mtim.tv_sec }
}

#[inline]
pub unsafe fn ngx_file_uniq(sb: *const ngx_file_info_t) -> ngx_file_uniq_t {
    unsafe { (*sb).st_ino as _ }
}

#[inline]
pub unsafe fn ngx_realpath(p: *const c_uchar, r: *mut c_uchar) -> *mut c_uchar {
    unsafe { libc::realpath(p.cast(), r.cast()).cast() }
}

#[inline]
pub unsafe fn ngx_getcwd(buf: *mut c_uchar, size: usize) -> usize {
    // Win32 version returns the length of the result as size_t.
    // Unix macro returns boolean (!= NULL), but for consistency we convert it to usize.
    if unsafe { libc::getcwd(buf.cast(), size) }.is_null() { 0 } else { 1 }
}

#[inline]
pub fn ngx_path_separator(c: c_uchar) -> bool {
    c == b'/'
}

#[inline]
pub unsafe fn ngx_close_dir(d: *mut ngx_dir_t) -> ngx_int_t {
    // crate::bindings::__dirstream (DIR) -> libc::DIR
    unsafe { libc::closedir((*d).dir.cast()) as _ }
}

#[inline]
pub unsafe fn ngx_create_dir(name: *const c_uchar, access: ngx_uint_t) -> ngx_int_t {
    unsafe { libc::mkdir(name.cast(), access as libc::mode_t) as _ }
}

#[inline]
pub unsafe fn ngx_delete_dir(name: *const c_uchar) -> ngx_int_t {
    unsafe { libc::rmdir(name.cast()) as _ }
}

#[inline]
pub fn ngx_dir_access(a: ngx_uint_t) -> ngx_uint_t {
    a | ((a & 0o444) >> 2)
}

#[inline]
pub unsafe fn ngx_de_name(dir: *const ngx_dir_t) -> *const c_uchar {
    unsafe { (*(*dir).de).d_name.as_ptr().cast() }
}

#[inline]
pub unsafe fn ngx_de_namelen(dir: *const ngx_dir_t) -> usize {
    cfg_if! {
        if #[cfg(ngx_feature = "have_d_namlen")] {
            unsafe { (*(*dir).de).d_namlen as _ }
        } else {
            unsafe { libc::strlen(ngx_de_name(dir).cast()) }
        }
    }
}

#[inline]
pub unsafe fn ngx_de_info(name: *const c_uchar, dir: *mut ngx_dir_t) -> ngx_int_t {
    unsafe {
        (*dir).set_type(0);
        libc::stat(name.cast(), (&raw mut (*dir).info).cast()) as _
    }
}

#[inline]
pub unsafe fn ngx_de_link_info(name: *const c_uchar, dir: *mut ngx_dir_t) -> ngx_int_t {
    unsafe { libc::lstat(name.cast(), (&raw mut (*dir).info).cast()) as _ }
}

cfg_if! {
    if #[cfg(ngx_feature = "have_d_type")] {
        #[inline]
        pub unsafe fn ngx_de_is_dir(dir: *const ngx_dir_t) -> bool {
            if unsafe { (*dir).type_() } != 0 {
                (unsafe { (*dir).type_() }) == libc::DT_DIR as _
            } else {
                unsafe { ngx_is_dir(&raw const (*dir).info) }
            }
        }

        #[inline]
        pub unsafe fn ngx_de_is_file(dir: *const ngx_dir_t) -> bool {
            if unsafe { (*dir).type_() } != 0 {
                (unsafe { (*dir).type_() }) == libc::DT_REG as _
            } else {
                unsafe { ngx_is_file(&raw const (*dir).info) }
            }
        }

        #[inline]
        pub unsafe fn ngx_de_is_link(dir: *const ngx_dir_t) -> bool {
            if unsafe { (*dir).type_() } != 0 {
                (unsafe { (*dir).type_() }) == libc::DT_LNK as _
            } else {
                unsafe { ngx_is_link(&raw const (*dir).info) }
            }
        }
    } else {
        #[inline]
        pub unsafe fn ngx_de_is_dir(dir: *const ngx_dir_t) -> bool {
            unsafe { ngx_is_dir(&raw const (*dir).info) }
        }

        #[inline]
        pub unsafe fn ngx_de_is_file(dir: *const ngx_dir_t) -> bool {
            unsafe { ngx_is_file(&raw const (*dir).info) }
        }

        #[inline]
        pub unsafe fn ngx_de_is_link(dir: *const ngx_dir_t) -> bool {
            unsafe { ngx_is_link(&raw const (*dir).info) }
        }
    }
}

#[inline]
pub unsafe fn ngx_de_access(dir: *const ngx_dir_t) -> ngx_uint_t {
    unsafe { ngx_file_access(&raw const (*dir).info) }
}

#[inline]
pub unsafe fn ngx_de_size(dir: *const ngx_dir_t) -> off_t {
    unsafe { ngx_file_size(&raw const (*dir).info) }
}

#[inline]
pub unsafe fn ngx_de_fs_size(dir: *const ngx_dir_t) -> off_t {
    unsafe { (*dir).info.st_size.max((*dir).info.st_blocks * 512) }
}

#[inline]
pub unsafe fn ngx_de_mtime(dir: *const ngx_dir_t) -> time_t {
    unsafe { ngx_file_mtime(&raw const (*dir).info) }
}

cfg_if! {
    if #[cfg(ngx_feature = "have_openat")] {
       #[inline]
        pub unsafe fn ngx_openat_file(
            fd: ngx_fd_t,
            name: *const c_uchar,
            mode: ngx_uint_t,
            create: ngx_uint_t,
            access: ngx_uint_t,
        ) -> ngx_fd_t {
            unsafe {
                libc::openat(fd, name.cast(), (mode | create) as c_int, access as c_uint) as _
            }
        }

        #[inline]
        pub unsafe fn ngx_file_at_info(
            fd: ngx_fd_t,
            name: *const c_uchar,
            sb: *mut ngx_file_info_t,
            flag: c_int
        ) -> ngx_int_t {
            unsafe { libc::fstatat(fd, name.cast(), sb.cast(), flag) as _ }
        }

    }
}

#[inline]
pub fn ngx_stdout() -> ngx_fd_t {
    libc::STDOUT_FILENO
}

#[inline]
pub fn ngx_stderr() -> ngx_fd_t {
    libc::STDERR_FILENO
}

/*
 * ngx_process.h
 */

/// Causes the calling thread to relinquish the CPU.
#[inline]
pub fn ngx_sched_yield() {
    unsafe { libc::sched_yield() };
}
