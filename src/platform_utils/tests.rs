use super::normalize_platform;

#[test]
fn normalize_platform_accepts_common_os_aliases() {
    assert_eq!(normalize_platform("darwin"), "macos");
    assert_eq!(normalize_platform("win32"), "windows");
    assert_eq!(normalize_platform("windows_nt"), "windows");
    assert_eq!(normalize_platform("linux"), "linux");
}
