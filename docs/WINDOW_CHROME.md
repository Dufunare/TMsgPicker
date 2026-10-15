# Unified window chrome

Each application WebView has exactly one window shell with a static or animated background. The titlebar is transparent and is a sibling of the business content. The media viewer bypasses the shell.

## Responsibilities

- `src/window/windowControls.ts`: Tauri window adapter only; native closing always dispatches the existing Rust `CloseRequested` handler.
- `src/components/WindowTitlebar.vue`: window gestures and accessible controls (no account or query state).
- `src/components/WindowShell.vue`: layout orchestration for main, settings and login pages.
- `src/theme/window-chrome.css`: frame and titlebar appearance; `glass.css`: one animated gradient per window.
- Business views render in `.window-body`; they do not own a second animated background.

## Desktop acceptance checklist

- Verify draggable areas, non-draggable controls, double-click maximizing, minimize, restore, keyboard focus, maximize resize limits and Windows snap/edge behavior.
- Close main with pending forwards: existing safe-close confirmation must still appear.
- Close login during authentication: cleanup must still run; close settings with unsaved preview: preview must roll back.
- Test main/settings/login, light/dark/glass and motion on/off; test OS reduced motion.
- Check narrow window layout and 125%/150% Windows DPI.

Rust/Tauri Windows rendering and glass visuals require real Windows QA; CI-only front-end tests cannot validate native caption behavior.
