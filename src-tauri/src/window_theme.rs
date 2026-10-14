use serde::Deserialize;
#[derive(Deserialize)]
struct Theme {
    background: [u8; 3],
    text: [u8; 3],
}
fn theme() -> Theme {
    serde_json::from_str(include_str!("../../src/theme.json")).expect("Valid bundled theme")
}
fn colorref([red, green, blue]: [u8; 3]) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}
pub(super) fn apply(window: &tauri::WebviewWindow) {
    let theme = theme();
    let [r, g, b] = theme.background;
    let _ = window.set_background_color(Some(tauri::window::Color(r, g, b, 255)));
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        #[link(name = "dwmapi")]
        unsafe extern "system" {
            fn DwmSetWindowAttribute(
                hwnd: *mut std::ffi::c_void,
                attribute: u32,
                value: *const std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        // Unsupported attributes return HRESULT failure: preserve native fallback.
        for (attribute, color) in [
            (35, colorref(theme.background)),
            (34, 0xffff_fffe),
            (36, colorref(theme.text)),
        ] {
            unsafe {
                DwmSetWindowAttribute(hwnd.0, attribute, (&color as *const u32).cast(), 4);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = (theme.text, colorref(theme.background));
}
/// Apply the active appearance to the Windows native caption and WebView background.
/// System mode follows the current OS preference on Windows; otherwise use the light fallback.
pub(super) fn apply_appearance(
    window: &tauri::WebviewWindow,
    appearance: &crate::settings::AppearancePreferences,
) {
    let dark = appearance.theme == "dark" || (appearance.theme == "system" && system_dark());
    let [r, g, b] = if dark { [20, 27, 39] } else { [246, 248, 252] };
    let text = if dark { [230, 238, 248] } else { [38, 55, 75] };
    let _ = window.set_background_color(Some(tauri::window::Color(r, g, b, 255)));
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        #[link(name = "dwmapi")]
        unsafe extern "system" {
            fn DwmSetWindowAttribute(
                hwnd: *mut std::ffi::c_void,
                attribute: u32,
                value: *const std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        for (attribute, color) in [
            (35, colorref([r, g, b])),
            (34, 0xffff_fffe),
            (36, colorref(text)),
        ] {
            unsafe {
                DwmSetWindowAttribute(hwnd.0, attribute, (&color as *const u32).cast(), 4);
            }
        }
        let use_dark: i32 = if dark { 1 } else { 0 };
        unsafe {
            DwmSetWindowAttribute(hwnd.0, 20, (&use_dark as *const i32).cast(), 4);
        }
    }
    #[cfg(not(windows))]
    let _ = text;
}
#[cfg(windows)]
fn system_dark() -> bool {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(key: isize, subkey: *const u16, options: u32, access: u32, out: *mut isize) -> i32;
        fn RegQueryValueExW(key: isize, name: *const u16, reserved: *mut u32, kind: *mut u32, data: *mut u8, size: *mut u32) -> i32;
        fn RegCloseKey(key: isize) -> i32;
    }
    let encode = |s: &str| std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let path = encode("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = encode("AppsUseLightTheme");
    let mut key = 0_isize;
    unsafe {
        if RegOpenKeyExW((0x80000001_u32 as i32) as isize, path.as_ptr(), 0, 0x20019, &mut key) != 0 {
            return false;
        }
        let mut value = 1_u32;
        let mut size = 4_u32;
        let ok = RegQueryValueExW(key, name.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), (&mut value as *mut u32).cast(), &mut size) == 0;
        RegCloseKey(key);
        ok && value == 0
    }
}
#[cfg(not(windows))]
fn system_dark() -> bool { false }
#[cfg(test)]
mod tests {
    #[test]
    fn colorref_is_bgr_and_theme_is_shared_with_webview() {
        assert_eq!(super::colorref([0x12, 0x34, 0x56]), 0x563412);
        assert_eq!(super::theme().background, [246, 248, 251]);
    }
}
