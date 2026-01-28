use ngx::ffi;

#[test]
fn test_os_symbols() {
    #[cfg(ngx_os = "freebsd")]
    assert_eq!(ffi::NGX_FREEBSD, 1);

    #[cfg(ngx_os = "linux")]
    assert_eq!(ffi::NGX_LINUX, 1);

    #[cfg(ngx_os = "darwin")]
    assert_eq!(ffi::NGX_DARWIN, 1);
}

#[test]
fn test_feature_symbols() {
    let ev: ffi::ngx_event_t = unsafe { core::mem::zeroed() };

    assert_eq!(ev.available, 0);

    #[cfg(ngx_feature = "have_kqueue")]
    assert_eq!(ev.kq_errno, 0);
}
