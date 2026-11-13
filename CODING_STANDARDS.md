# Coding Standards

Review rules requiring cross-file and architectural judgement. Mechanical formatting and lints are enforced by `cargo fmt --all -- --check` and `cargo clippy --all-targets --locked -- -D warnings`.

## Cross-file setting completeness

Every user-facing visual or behavior setting in `Config` (`src/config.rs`) must reach all four surfaces:

1. **`src/config.rs`**: `Config` field, `parse()` with fallback default, `DEFAULT` template comment, `save_setting()` support, and a round-trip test in `mod tests`.
2. **`src/tray.rs`**: `TrayMsg` variant and StatusNotifierItem menu item or submenu radio so the user can toggle it live without editing TOML.
3. **`src/main.rs`**: live `TrayMsg` handler that updates `State`, re-applies CSS/classes immediately, and persists via `config::save_setting`.
4. **`fn demo` (`src/main.rs`) & `tools/shot.sh`**: demo token or CLI flag so the state can be captured deterministically in isolation without touching the user's running session.

## Retain-until-ready for async GTK textures

Never clear or hide an existing `gtk::Picture` or paintable before its replacement finishes loading off-thread:

- Keep the previous cover art or thumbnail visible while fetching or decoding the new `mpris:artUrl` or file path.
- Only hide the widget when the player explicitly reports an empty `artUrl` or no active track.
- When a `file://` path fails to read (for example, a Snap browser's isolated `/tmp` in Ubuntu/Kubuntu), retain the currently displayed texture rather than blanking the cover.
