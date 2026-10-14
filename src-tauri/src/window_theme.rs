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
#[cfg(test)]
mod tests {
    #[test]
    fn colorref_is_bgr_and_theme_is_shared_with_webview() {
        assert_eq!(super::colorref([0x12, 0x34, 0x56]), 0x563412);
        assert_eq!(super::theme().background, [246, 248, 251]);
    }
}
