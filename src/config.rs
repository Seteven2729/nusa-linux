//! Settings in ~/.config/nusa/config.toml. Read once at startup; "Reload" in the tray
//! restarts Nusa to apply edits.

use crate::*;
use std::sync::OnceLock;

/// Written on first run. Every key is optional; anything missing or invalid falls back to the default.
const TEMPLATE: &str = r##"# Nusa settings. After editing, choose "Reload" in the Nusa tray menu.

# Language of the notch and tray menu: "id" (Bahasa Indonesia) or "en" (English).
language = "id"

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
mixer = true        # volume mixer page: output and per-app volume
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
# "nusa" (dark blue), "midnight" (black), "rose", "forest", or "custom" to use `gradient` below.
# follow_cover / follow_wallpaper tint over the theme while they apply; turn them off to
# see the theme's own gradient all the time.
theme = "nusa"
# Only used with theme = "custom": notch gradient, top-left to bottom-right, as #rrggbb.
gradient = ["#020510", "#061338", "#0c225c"]
follow_cover = true      # tint with the album cover while music plays
follow_wallpaper = true  # tint with the Plasma wallpaper when idle
"##;

pub(crate) struct Config {
    pub(crate) english: bool,
    pub(crate) minimal: bool,
    pub(crate) mascot: bool,
    pub(crate) lyrics: bool,
    pub(crate) romanize: bool,
    pub(crate) gestures: bool,
    pub(crate) stats: bool,
    pub(crate) clipboard: bool,
    pub(crate) mic: bool,
    pub(crate) mixer: bool,
    pub(crate) camera: bool,
    pub(crate) phone: bool,
    pub(crate) downloads: bool,
    pub(crate) battery: bool,
    pub(crate) network: bool,
    pub(crate) bluetooth: bool,
    pub(crate) mascot_awake: u32,
    pub(crate) mascot_sleep: u32,
    pub(crate) gradient: [String; 3],
    /// Secondary text (artist, stats, times).
    pub(crate) muted: String,
    /// Equalizer bars and progress bars.
    pub(crate) accent: String,
    /// Slider fill and knob, play button, active toggles.
    pub(crate) highlight: String,
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
    let theme_name = doc.get("colors").and_then(|c| c.get("theme")?.as_str()).unwrap_or("nusa");
    let theme = THEMES.iter().find(|t| t.0 == theme_name).unwrap_or(&THEMES[0]);
    let gradient = std::array::from_fn(|i| {
        let custom = doc.get("colors").and_then(|c| c.get("gradient")?.as_array()?.get(i)?.as_str().map(str::to_owned));
        match custom.filter(|c| theme_name == "custom" && is_hex_color(c)) {
            Some(c) => c,
            None => theme.1[i].to_owned(),
        }
    });
    Config {
        english: item("", "language").and_then(|i| i.as_str().map(|l| l == "en")).unwrap_or(false),
        minimal: item("", "minimal").and_then(|i| i.as_bool()).unwrap_or(false),
        mascot: flag("", "mascot"),
        lyrics: flag("features", "lyrics"),
        romanize: flag("features", "romanize"),
        gestures: flag("features", "gestures"),
        stats: flag("features", "stats"),
        clipboard: flag("features", "clipboard"),
        mic: flag("features", "mic"),
        mixer: flag("features", "mixer"),
        camera: flag("features", "camera"),
        phone: flag("features", "phone"),
        downloads: flag("features", "downloads"),
        battery: flag("features", "battery"),
        network: flag("features", "network"),
        bluetooth: flag("features", "bluetooth"),
        mascot_awake: secs("mascot_awake", 30),
        mascot_sleep: secs("mascot_sleep", 180),
        gradient,
        muted: theme.2.to_owned(),
        accent: theme.3.to_owned(),
        highlight: theme.4.to_owned(),
        follow_cover: flag("colors", "follow_cover"),
        follow_wallpaper: flag("colors", "follow_wallpaper"),
    }
}

/// (name, gradient, muted text, accent, highlight). "custom" keeps the nusa text and control colors.
const THEMES: [(&str, [&str; 3], &str, &str, &str); 5] = [
    ("nusa", ["#020510", "#061338", "#0c225c"], "#b9cdf5", "#8fd8ff", "#ffffff"),
    ("midnight", ["#000000", "#0b0b0d", "#1c1c1f"], "#a1a1aa", "#e5e5ea", "#ffffff"),
    ("rose", ["#12030a", "#3a0b24", "#6b1740"], "#f5b9d3", "#ff8fc4", "#ffa3d1"),
    ("forest", ["#020a05", "#0b2a17", "#155e34"], "#b9f5cf", "#8fffb8", "#a3ffc4"),
    ("custom", ["#020510", "#061338", "#0c225c"], "#b9cdf5", "#8fd8ff", "#ffffff"),
];

/// Indonesian or English text, per `language` in config.toml.
pub(crate) fn tr(id: &'static str, en: &'static str) -> &'static str {
    if cfg().english { en } else { id }
}

/// Colors go straight into CSS, so only accept #rrggbb.
fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Theme names in the order the tray lists them.
pub(crate) fn theme_names() -> Vec<&'static str> {
    THEMES.iter().map(|t| t.0).collect()
}

/// `[colors] theme` as written in the file, "nusa" when unset or unknown.
pub(crate) fn current_theme_name() -> &'static str {
    let text = std::fs::read_to_string(config_path()).unwrap_or_default();
    let doc: toml_edit::DocumentMut = text.parse().unwrap_or_default();
    let name = doc.get("colors").and_then(|c| c.get("theme")?.as_str().map(str::to_owned)).unwrap_or_default();
    THEMES.iter().find(|t| t.0 == name).map_or("nusa", |t| t.0)
}

/// The settings as they are in the file now (cfg() keeps the ones Nusa started with).
pub(crate) fn read_config() -> Config {
    parse_config(&std::fs::read_to_string(config_path()).unwrap_or_default())
}

/// Store `[colors] theme` from the tray, keeping the file's comments and layout.
pub(crate) fn save_theme(name: &str) {
    let path = config_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| TEMPLATE.to_owned());
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else { return };
    if !doc.contains_table("colors") {
        doc["colors"] = toml_edit::table();
    }
    doc["colors"]["theme"] = toml_edit::value(name);
    let _ = std::fs::create_dir_all(path.parent().unwrap()).and_then(|_| std::fs::write(&path, doc.to_string()));
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
    assert!(!c.english && c.mixer);

    let c = parse_config("minimal = true\nlanguage = \"en\"\n[features]\nclipboard = false\n[timing]\nmascot_awake = 1\n[colors]\ntheme = \"custom\"\ngradient = [\"#ff0000\", \"red; }\", \"#00ff00\"]\n");
    assert!(c.english);
    assert!(c.minimal && !c.clipboard && c.lyrics, "unset keys keep their defaults");
    assert_eq!(c.mascot_awake, 5, "timing is clamped");
    assert_eq!(c.gradient, ["#ff0000", "#061338", "#00ff00"].map(String::from), "bad colors fall back");

    assert!(parse_config("this is [ not toml").lyrics, "a broken file means defaults");

    let rose = parse_config("[colors]\ntheme = \"rose\"\ngradient = [\"#ff0000\", \"#ff0000\", \"#ff0000\"]\n");
    assert_eq!((rose.gradient[1].as_str(), rose.accent.as_str()), ("#3a0b24", "#ff8fc4"), "a preset ignores gradient");
    assert_eq!(rose.highlight, "#ffa3d1");

    // Every theme fills every color token in the stylesheet.
    for (name, ..) in THEMES {
        let css = themed_css(&parse_config(&format!("[colors]\ntheme = \"{name}\"\n")));
        let leftover = css.match_indices('{').any(|(i, _)| css[i + 1..].starts_with(|ch: char| ch.is_ascii_uppercase()));
        assert!(!leftover, "theme {name} left a color token unfilled");
    }
    assert_eq!(parse_config("[colors]\ntheme = \"neon\"\n").gradient[0], "#020510", "unknown theme falls back to nusa");
}
