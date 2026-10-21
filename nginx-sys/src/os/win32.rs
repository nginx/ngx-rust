#![allow(missing_docs, clippy::missing_safety_doc)]
use core::ffi::c_uchar;

use windows_sys::Win32;
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;

use crate::bindings::{
    ngx_dir_t, ngx_fd_t, ngx_file_info_t, ngx_file_uniq_t, ngx_int_t, ngx_uint_t, off_t, time_t,
};

/*
 * ngx_config.h
 */

/// Returns a non cryptograhpically-secure pseudo-random integer.
#[inline]
pub fn ngx_random() -> core::ffi::c_long {
    use libc::rand;

    unsafe {
        // Emulate random() as Microsoft CRT does not provide it.
        // rand() should be thread-safe in the multi-threaded CRT we link to, but will not be seeded
        // outside of the main thread.
        let x: u32 = ((rand() as u32) << 16) ^ ((rand() as u32) << 8) ^ (rand() as u32);
        (0x7fffffff & x) as _
    }
}

/*
 * ngx_files.h
 */

pub const NGX_INVALID_FILE: ngx_fd_t = Win32::Foundation::INVALID_HANDLE_VALUE;

#[inline]
pub unsafe fn ngx_close_file(fd: ngx_fd_t) -> ngx_int_t {
    unsafe { Win32::Foundation::CloseHandle(fd) as _ }
}

#[inline]
pub unsafe fn ngx_fd_info(fd: ngx_fd_t, fi: *mut ngx_file_info_t) -> ngx_int_t {
    unsafe { Win32::Storage::FileSystem::GetFileInformationByHandle(fd, fi.cast()) as _ }
}

pub use crate::bindings::ngx_file_info as ngx_link_info;

#[inline]
pub unsafe fn ngx_is_dir(fi: *const ngx_file_info_t) -> bool {
    unsafe { (*fi).dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 }
}

#[inline]
pub unsafe fn ngx_is_file(fi: *const ngx_file_info_t) -> bool {
    unsafe { (*fi).dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 }
}

#[inline]
pub unsafe fn ngx_is_link(_fi: *const ngx_file_info_t) -> bool {
    false
}

#[inline]
pub unsafe fn ngx_is_exec(_fi: *const ngx_file_info_t) -> bool {
    false
}

#[inline]
pub unsafe fn ngx_file_access(_fi: *const ngx_file_info_t) -> ngx_uint_t {
    0
}

#[inline]
pub unsafe fn ngx_file_size(fi: *const ngx_file_info_t) -> off_t {
    unsafe { ((*fi).nFileSizeHigh as off_t) << 32 | (*fi).nFileSizeLow as off_t }
}

pub use ngx_file_size as ngx_file_fs_size;

#[inline]
pub unsafe fn ngx_file_uniq(fi: *const ngx_file_info_t) -> ngx_file_uniq_t {
    unsafe {
        ((*fi).nFileIndexHigh as ngx_file_uniq_t) << 32 | (*fi).nFileIndexLow as ngx_file_uniq_t
    }
}

#[inline]
pub unsafe fn ngx_file_mtime(fi: *const ngx_file_info_t) -> time_t {
    let ft = unsafe { (*fi).ftLastWriteTime };
    let mtime = (ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64;
    ((mtime as i64 - 116444736000000000) / 10000000) as time_t
}

#[inline]
pub fn ngx_path_separator(c: c_uchar) -> bool {
    c == b'/' || c == b'\\'
}

#[inline]
pub fn ngx_dir_access(a: ngx_uint_t) -> ngx_uint_t {
    a
}

#[inline]
pub unsafe fn ngx_de_name(dir: *const ngx_dir_t) -> *const c_uchar {
    unsafe { (*dir).name }
}

#[inline]
pub unsafe fn ngx_de_namelen(dir: *const ngx_dir_t) -> usize {
    unsafe { (*dir).namelen }
}

#[inline]
pub unsafe fn ngx_de_is_dir(dir: *const ngx_dir_t) -> bool {
    unsafe { (*dir).finddata.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 }
}

#[inline]
pub unsafe fn ngx_de_is_file(dir: *const ngx_dir_t) -> bool {
    unsafe { (*dir).finddata.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 }
}

#[inline]
pub unsafe fn ngx_de_is_link(_dir: *const ngx_dir_t) -> bool {
    false
}

#[inline]
pub unsafe fn ngx_de_access(_dir: *const ngx_dir_t) -> ngx_uint_t {
    0
}

#[inline]
pub unsafe fn ngx_de_size(dir: *const ngx_dir_t) -> off_t {
    unsafe {
        ((*dir).finddata.nFileSizeHigh as off_t) << 32 | (*dir).finddata.nFileSizeLow as off_t
    }
}

pub use ngx_de_size as ngx_de_fs_size;

#[inline]
pub unsafe fn ngx_de_mtime(dir: *const ngx_dir_t) -> time_t {
    let ft = unsafe { (*dir).finddata.ftLastWriteTime };
    let mtime = (ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64;
    ((mtime as i64 - 116444736000000000) / 10000000) as time_t
}

#[inline]
pub fn ngx_stdout() -> ngx_fd_t {
    unsafe { Win32::System::Console::GetStdHandle(Win32::System::Console::STD_OUTPUT_HANDLE) }
}

#[inline]
pub fn ngx_stderr() -> ngx_fd_t {
    unsafe { Win32::System::Console::GetStdHandle(Win32::System::Console::STD_ERROR_HANDLE) }
}

/*
 * ngx_process.h
 */

/// Causes the calling thread to relinquish the CPU.
#[inline]
pub fn ngx_sched_yield() {
    unsafe { crate::bindings::SwitchToThread() };
}
