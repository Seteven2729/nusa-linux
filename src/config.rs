//! Settings in ~/.config/nusa/config.toml. Read once at startup; "Reload" in the tray
//! restarts Nusa to apply edits.

use crate::*;
use std::sync::OnceLock;

/// Written on first run. Every key is optional; anything missing or invalid falls back to the default.
const TEMPLATE: &str = r##"# Nusa settings. After editing, choose "Reload" in the Nusa tray menu.

# Closed notch is a thin line until something happens. Also toggled from the tray.
minimal = false
# Eyes that appear when you're idle. Also toggled from the tray.
mascot = true

[features]
lyrics = true       # synced lyrics from lrclib.net
romanize = true     # romanized line under Korean/Japanese/Chinese/Cyrillic lyrics
gestures = true     # scroll = volume, swipe = next/previous, middle click = play/pause
stats = true        # CPU/RAM/temperature in the idle view
clipboard = true    # clipboard button in the idle view (Klipper)
mic = true          # orange dot while an app records from a microphone
camera = true       # green dot while a camera is on, red dot while the screen is shared
phone = true        # phone notifications via KDE Connect
downloads = true    # download progress bar
battery = true      # charger plugged/unplugged alerts
network = true      # Wi-Fi connected/disconnected alerts
bluetooth = true    # headphone connected/disconnected alerts

[timing]
mascot_awake = 30   # seconds idle before the eyes appear
mascot_sleep = 180  # seconds idle before they fall asleep

[colors]
# Notch gradient, top-left to bottom-right, as #rrggbb.
gradient = ["#020510", "#061338", "#0c225c"]
follow_cover = true      # tint with the album cover while music plays
follow_wallpaper = true  # tint with the Plasma wallpaper when idle
"##;

pub(crate) struct Config {
    pub(crate) minimal: bool,
    pub(crate) mascot: bool,
    pub(crate) lyrics: bool,
    pub(crate) romanize: bool,
    pub(crate) gestures: bool,
    pub(crate) stats: bool,
    pub(crate) clipboard: bool,
    pub(crate) mic: bool,
    pub(crate) camera: bool,
    pub(crate) phone: bool,
    pub(crate) downloads: bool,
    pub(crate) battery: bool,
    pub(crate) network: bool,
    pub(crate) bluetooth: bool,
    pub(crate) mascot_awake: u32,
    pub(crate) mascot_sleep: u32,
    pub(crate) gradient: [String; 3],
    pub(crate) follow_cover: bool,
    pub(crate) follow_wallpaper: bool,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// The settings Nusa started with.
pub(crate) fn cfg() -> &'static Config {
    CONFIG.get_or_init(load_config)
}

pub(crate) fn config_path() -> PathBuf {
    glib::user_config_dir().join("nusa/config.toml")
}

fn load_config() -> Config {
    let path = config_path();
    if !path.exists() {
        let _ = std::fs::create_dir_all(path.parent().unwrap()).and_then(|_| std::fs::write(&path, TEMPLATE));
        migrate_flag_files();
    }
    parse_config(&std::fs::read_to_string(&path).unwrap_or_default())
}

/// Before config.toml, the two tray toggles were empty marker files. Carry them over once.
fn migrate_flag_files() {
    let dir = glib::user_config_dir().join("nusa");
    if std::fs::remove_file(dir.join("minimal")).is_ok() {
        save_setting("minimal", true);
    }
    if std::fs::remove_file(dir.join("no-mascot")).is_ok() {
        save_setting("mascot", false);
    }
}

fn parse_config(text: &str) -> Config {
    // A broken file means defaults, not a notch that won't start.
    let doc: toml_edit::DocumentMut = text.parse().or_else(|_| TEMPLATE.parse()).unwrap();
    let def: toml_edit::DocumentMut = TEMPLATE.parse().unwrap();
    let item = |table: &str, key: &str| -> Option<toml_edit::Item> {
        let pick = |d: &toml_edit::DocumentMut| {
            if table.is_empty() { d.get(key).cloned() } else { d.get(table)?.get(key).cloned() }
        };
        pick(&doc).or_else(|| pick(&def))
    };
    // Every feature flag defaults to on; a value of the wrong type counts as unset.
    let flag = |table: &str, key: &str| item(table, key).and_then(|i| i.as_bool()).unwrap_or(true);
    let secs = |key: &str, default: u32| {
        item("timing", key).and_then(|i| i.as_integer()).map(|n| n.clamp(5, 3600) as u32).unwrap_or(default)
    };
    let defaults = ["#020510", "#061338", "#0c225c"];
    let gradient = std::array::from_fn(|i| {
        doc.get("colors")
            .and_then(|c| c.get("gradient")?.as_array()?.get(i)?.as_str().map(str::to_owned))
            .filter(|c| is_hex_color(c))
            .unwrap_or_else(|| defaults[i].to_owned())
    });
    Config {
        minimal: item("", "minimal").and_then(|i| i.as_bool()).unwrap_or(false),
        mascot: flag("", "mascot"),
        lyrics: flag("features", "lyrics"),
        romanize: flag("features", "romanize"),
        gestures: flag("features", "gestures"),
        stats: flag("features", "stats"),
        clipboard: flag("features", "clipboard"),
        mic: flag("features", "mic"),
        camera: flag("features", "camera"),
        phone: flag("features", "phone"),
        downloads: flag("features", "downloads"),
        battery: flag("features", "battery"),
        network: flag("features", "network"),
        bluetooth: flag("features", "bluetooth"),
        mascot_awake: secs("mascot_awake", 30),
        mascot_sleep: secs("mascot_sleep", 180),
        gradient,
        follow_cover: flag("colors", "follow_cover"),
        follow_wallpaper: flag("colors", "follow_wallpaper"),
    }
}

/// Colors go straight into CSS, so only accept #rrggbb.
fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Store a top-level toggle from the tray, keeping the file's comments and layout.
pub(crate) fn save_setting(key: &str, on: bool) {
    let path = config_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| TEMPLATE.to_owned());
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else { return };
    doc[key] = toml_edit::value(on);
    let _ = std::fs::create_dir_all(path.parent().unwrap()).and_then(|_| std::fs::write(&path, doc.to_string()));
}

#[test]
fn config_parsing() {
    let c = parse_config(TEMPLATE);
    assert!(!c.minimal && c.mascot && c.lyrics && c.follow_wallpaper);
    assert_eq!((c.mascot_awake, c.mascot_sleep), (30, 180));
    assert_eq!(c.gradient[0], "#020510");

    let c = parse_config("minimal = true\n[features]\nclipboard = false\n[timing]\nmascot_awake = 1\n[colors]\ngradient = [\"#ff0000\", \"red; }\", \"#00ff00\"]\n");
    assert!(c.minimal && !c.clipboard && c.lyrics, "unset keys keep their defaults");
    assert_eq!(c.mascot_awake, 5, "timing is clamped");
    assert_eq!(c.gradient, ["#ff0000", "#061338", "#00ff00"].map(String::from), "bad colors fall back");

    assert!(parse_config("this is [ not toml").lyrics, "a broken file means defaults");
}
