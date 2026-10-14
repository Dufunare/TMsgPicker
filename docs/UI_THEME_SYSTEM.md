# TMsgPicker UI Design System

The main workspace remains a **three-pane, rounded-card desktop layout** with draggable invisible splitters. Layout ratios remain **ephemeral**, and account/session semantics are unchanged.

## Structure

- `src/theme/tokens.css`: palette and the light/dark semantic tokens, elevation levels, spacing.
- `src/theme/components.css`: reusable buttons, inputs, selects, checkboxes, scrollbars, badges, notices, message metrics, filter nodes and dialogs.
- `src/workspace.css`: only layout, dimensions, sticky areas, and pane-specific structure. Colors reference semantic tokens.
- `src/theme/appearance.ts`: typed appearance preferences, CSS-variable application and cross-window event synchronization.
- `src/theme/appearance-settings.css`: appearance editor layout.
- `src/components/AppearanceSettings.vue`: live appearance preview, save and defaults.
- `src-tauri/src/settings.rs`: persisted, validated `ApplicationConfig.appearance`.
- `src-tauri/src/window_theme.rs`: Windows native caption background/text synced to theme when supported.

## Preferences

| Setting | Choices |
| --- | --- |
| Theme | Follow system, Light, Dark |
| Accent | Six-digit HEX color |
| Density | Comfortable, Compact |
| Corner radius | 6–22px |
| Font scale | 85–120% |
| Elevation | Flat, Soft, Raised |
| Scrollbar | Slim, Standard |

Appearance settings are **app-wide**, not per Telegram account, and are stored alongside non-secret application configuration. Changing them never restarts TDLib clients or logs out accounts. Preferences are validated in Rust and TypeScript, and distributed to every open Tauri window by the `appearance-changed` event.

## Extending the UI

Use only semantic design tokens in new component styles: `--surface`, `--canvas`, `--text`, `--text-muted`, `--border`, `--accent`, `--selected`, `--radius`, `--shadow`, etc. New controls should inherit `src/theme/components.css` instead of adding duplicate hardcoded buttons or scrollbar implementations.

## Compatibility / caveats

- Prior application config without `appearance` deserializes with default appearance preferences.
- Previously deprecated persistent split ratios are **not reintroduced**.
- CSS follows OS dark/light changes when Theme is set to Follow system. Windows caption updates when selecting/saving a theme in the app; an OS theme change while the app is open updates WebView CSS, and caption color may require reopening a window if no app theme-change event fires.
- Windows native GUI visual QA and native Rust checks are separate from the frontend GitHub Actions workflow; tests must not invoke actual Telegram authorization, logout, forwarding, or Session deletion.
