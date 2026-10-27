# nusa-linux

Notch overlay for Wayland desktops, in Rust with GTK4 and gtk4-layer-shell. Each `src/*.rs` opens with a `//!` line naming what it owns. Build, test, install and release steps live in `README.md`.

## Testing against fakes

The user runs Nusa all day as the systemd user service `nusa-linux.service` and plays music in Strawberry. Test with fakes so their session stays exactly as it was:

- Screenshot any notch state: `tools/shot.sh <out.png> "<NUSA_DEMO spec>" [--music] [--theme <name>]`. It uses a temporary config and folders, and stops and restarts the service itself.
- Ad-hoc runs use a debug build (`cargo build`) with `XDG_CONFIG_HOME=<temp dir>`, `NUSA_ONLY_PLAYER=fake`, `NUSA_DEMO=...` (options on `fn demo` in `src/main.rs`) and `tools/fake_player.py`. Release builds ignore these variables.
- Before changing an app's volume, playback or settings, confirm by name that it is the fake one.
- After `systemctl --user stop nusa-linux.service`, start it again before finishing.
- Run tests as `rtk proxy cargo test`; the rtk filter hides the result lines.

## Commits and releases

- Commits use the repo's configured noreply email and end with the message body, with no attribution trailer.
- Push to the `repo` remote.
- Release steps: README, section "Rilis versi baru". CI commits the PKGBUILD bump after each release, so `git pull` before the next commit.
