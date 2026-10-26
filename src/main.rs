use futures_util::StreamExt;
use gtk::{gio, glib, prelude::*};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};
use std::path::PathBuf;
use zbus::{proxy, Connection, MatchRule, MessageStream, ObjectPath, OwnedObjectPath, OwnedVariant};

#[proxy(interface = "org.mpris.MediaPlayer2.Player", default_path = "/org/mpris/MediaPlayer2")]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn set_position(&self, track_id: &ObjectPath<'_>, position: i64) -> zbus::Result<()>;
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedVariant>>;
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;
}

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait Battery {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait Network {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(interface = "org.freedesktop.NetworkManager.Connection.Active", default_service = "org.freedesktop.NetworkManager")]
trait ActiveConnection {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<String>;
}

#[proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait BtDevice {
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon(&self) -> zbus::Result<String>;
}

#[proxy(interface = "org.bluez.Battery1", default_service = "org.bluez")]
trait BtBattery {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<u8>;
}

#[proxy(interface = "org.kde.klipper.klipper", default_service = "org.kde.klipper", default_path = "/klipper")]
// KDE's D-Bus APIs use camelCase names; zbus would default to PascalCase, so name them explicitly.
trait Klipper {
    #[zbus(name = "getClipboardHistoryMenu")]
    fn get_clipboard_history_menu(&self) -> zbus::Result<Vec<String>>;
    #[zbus(name = "setClipboardContents")]
    fn set_clipboard_contents(&self, s: &str) -> zbus::Result<()>;
    #[zbus(signal, name = "clipboardHistoryUpdated")]
    fn clipboard_history_updated(&self) -> zbus::Result<()>;
}

#[proxy(interface = "org.kde.kdeconnect.device.notifications.notification", default_service = "org.kde.kdeconnect")]
trait PhoneNotification {
    #[zbus(property, name = "appName")]
    fn app_name(&self) -> zbus::Result<String>;
    #[zbus(property, name = "title")]
    fn title(&self) -> zbus::Result<String>;
    #[zbus(property, name = "text")]
    fn text(&self) -> zbus::Result<String>;
    #[zbus(property, name = "silent")]
    fn silent(&self) -> zbus::Result<bool>;
}

const CSS: &str = "
window { background: transparent; }
.notch {
  background: linear-gradient(160deg, #020510 0%, #061338 65%, #0c225c 100%); color: #fff;
  font-family: Inter, 'Noto Sans', sans-serif;
  border-radius: 0 0 20px 20px; padding: 5px 14px 7px; min-width: 150px;
  margin: 0 14px 14px; box-shadow: 0 6px 20px rgba(4,12,40,0.6);
  transition: border-radius 180ms ease, padding 180ms ease;
}
.notch.open { border-radius: 0 0 28px 28px; padding: 8px 16px 14px; }
.notch.thin { padding: 0; min-height: 5px; box-shadow: none; }
.compact { font-size: 13px; font-weight: 600; }
.mini-art { border-radius: 5px; }
.art { border-radius: 12px; }
.title { font-size: 15px; font-weight: 700; }
.title.clock { font-size: 30px; font-weight: 700; font-feature-settings: 'tnum'; }
.artist { font-size: 12px; color: #b9cdf5; }
.time { font-size: 11px; color: #b9cdf5; font-feature-settings: 'tnum'; }
.lyric { font-size: 13px; font-weight: 600; color: #fff; opacity: 0.9; margin-top: 2px; }

.eq { min-height: 14px; }
.eq > box { background: #8fd8ff; border-radius: 2px; min-width: 3px; min-height: 3px; }

.notch scale { padding: 0; min-width: 210px; }
.notch scale trough { min-height: 4px; border-radius: 2px; background: rgba(255,255,255,0.22); border: none; }
.notch scale highlight { background: #fff; border-radius: 2px; border: none; margin: 0; min-width: 0; }
.notch scale slider { min-width: 0; min-height: 0; margin: 0; background: #fff; border: none; box-shadow: none; transition: all 120ms ease; }
.notch scale:hover slider { min-width: 12px; min-height: 12px; margin: -4px; }

.notch button { background: none; border: none; box-shadow: none; color: #fff; min-width: 36px; min-height: 36px; border-radius: 999px; padding: 0; }
.notch button:hover { background: rgba(255,255,255,0.15); }
.notch button.play { background: #fff; color: #061338; }
.notch button.play:hover { background: #dce8ff; }
.notch button.lyrics-toggle { min-width: 30px; min-height: 30px; opacity: 0.6; }
.notch button.lyrics-toggle:checked { opacity: 1; background: rgba(255,255,255,0.15); }

.notch scrolledwindow, .notch list, .notch row { background: none; }
.notch row { padding: 0; border-radius: 6px; }
.notch row:hover { background: rgba(255,255,255,0.08); }
.lrc { font-size: 13px; padding: 3px 6px; opacity: 0.45; }
.lrc.now { opacity: 1; font-weight: 700; }
.rom { font-size: 11px; opacity: 0.6; }
.mic { color: #ff9f0a; font-size: 10px; }
.stats { font-size: 11px; color: #b9cdf5; font-feature-settings: 'tnum'; }
.notch button.clip { min-height: 26px; padding: 0 10px; border-radius: 8px; background: rgba(255,255,255,0.06); font-size: 12px; }
.notch button.clip:hover { background: rgba(255,255,255,0.14); }
";

#[derive(Clone)]
struct Ui {
    notch: gtk::Box,
    seek: gtk::Scale,
    time: gtk::Label,
    compact: gtk::Label,
    mini_art: gtk::Image,
    art: gtk::Image,
    eq: gtk::Box,
    lyric: gtk::Label,
    expanded: gtk::Revealer,
    title: gtk::Label,
    artist: gtk::Label,
    play: gtk::Button,
    buttons: gtk::Box,
    mascot: gtk::DrawingArea,
    lyrics_btn: gtk::ToggleButton,
    lyrics_list: gtk::ListBox,
    lyrics_scroll: gtk::ScrolledWindow,
    lyric_rom: gtk::Label,
    mic: gtk::Label,
    stats: gtk::Label,
    clips: gtk::Box,
    accent: gtk::CssProvider,
}

/// What the idle mascot is doing; `None` means it is hidden and the clock shows.
#[derive(Default, Clone, Copy, PartialEq)]
enum Mood {
    #[default]
    None,
    Awake,
    Sleep,
    Wave,
}

#[derive(Default)]
struct State {
    player: Option<PlayerProxy<'static>>,
    playing: bool,
    track: String,
    lyrics: Vec<(i64, String)>,
    alert: Option<String>,
    /// Bumped per alert so an older alert's timeout doesn't clear a newer one.
    alert_seq: u32,
    track_id: Option<OwnedObjectPath>,
    minimal: bool,
    /// Slider target while the mouse is still held on the seek bar.
    seeking: Option<f64>,
    art_url: String,
    /// Index of the highlighted line in the full lyrics list.
    lyric_idx: Option<usize>,
    mood: Mood,
    blink: bool,
    mascot_off: bool,
    /// Romanized lyric lines, same order as `lyrics`; None where a line needs none.
    lyrics_rom: Vec<Option<String>>,
    /// Previous (busy, total) jiffies from /proc/stat, for the CPU percentage.
    cpu_prev: Option<(u64, u64)>,
    /// Apps recording from a microphone right now.
    mic_apps: Vec<String>,
}

type Shared = Rc<RefCell<State>>;

fn main() -> glib::ExitCode {
    let app = gtk::Application::new(Some("local.nusa"), Default::default());
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &gtk::Application) {
    // Launching again just re-activates the running instance; keep one notch.
    if app.active_window().is_some() {
        return;
    }
    let css = gtk::CssProvider::new();
    css.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = gtk::ApplicationWindow::new(app);
    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Top, true);
    window.set_namespace(Some("nusa"));
    // Non-resizable GTK windows follow their content both ways, so the notch shrinks back after closing.
    window.set_resizable(false);

    let compact = gtk::Label::builder()
        .css_classes(["compact"])
        .max_width_chars(32)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .hexpand(true)
        .build();
    // gtk::Image draws at a fixed pixel size; gtk::Picture would ask for the cover's full resolution.
    let picture = |size: i32, class: &str| {
        let p = gtk::Image::builder().pixel_size(size).css_classes([class]).build();
        p.set_overflow(gtk::Overflow::Hidden);
        p.set_valign(gtk::Align::Center);
        p
    };
    let mini_art = picture(20, "mini-art");
    let art = picture(56, "art");
    art.set_visible(false);
    // Three bouncing bars while music plays, like the "now playing" indicator on phones.
    let eq = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    eq.add_css_class("eq");
    eq.set_valign(gtk::Align::Center);
    for _ in 0..3 {
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.set_valign(gtk::Align::Center);
        eq.append(&bar);
    }
    let mascot = gtk::DrawingArea::builder().content_width(44).content_height(16).visible(false).build();
    mascot.set_hexpand(true);
    // Bars jump to random heights 4x a second. A CSS animation looked smoother but redrew
    // at 60 fps, which cost ~4% CPU for the whole time music played.
    let e = eq.clone();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        if e.is_visible() {
            let mut bar = e.first_child();
            while let Some(b) = bar {
                b.set_size_request(3, glib::random_int_range(3, 15));
                bar = b.next_sibling();
            }
        }
        glib::ControlFlow::Continue
    });
    // Orange dot while any app records from the microphone.
    let mic = gtk::Label::builder().label("●").css_classes(["mic"]).visible(false).build();
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    top.append(&mic);
    top.append(&mini_art);
    top.append(&compact);
    top.append(&mascot);
    top.append(&eq);

    let lyric = gtk::Label::builder()
        .css_classes(["lyric"])
        .visible(false)
        .max_width_chars(40)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let lyric_rom = gtk::Label::builder()
        .css_classes(["rom"])
        .visible(false)
        .max_width_chars(48)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let title = gtk::Label::builder()
        .css_classes(["title"])
        .xalign(0.0)
        .max_width_chars(28)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let artist = gtk::Label::builder()
        .css_classes(["artist"])
        .xalign(0.0)
        .max_width_chars(32)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let prev = gtk::Button::from_icon_name("media-skip-backward-symbolic");
    let play = gtk::Button::from_icon_name("media-playback-start-symbolic");
    play.add_css_class("play");
    let next = gtk::Button::from_icon_name("media-skip-forward-symbolic");
    let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    seek.set_hexpand(true);
    seek.set_valign(gtk::Align::Center);
    let time = gtk::Label::builder().css_classes(["time"]).build();
    let seek_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    seek_row.append(&seek);
    seek_row.append(&time);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    text.append(&title);
    text.append(&artist);
    let lyrics_btn = gtk::ToggleButton::builder()
        .icon_name("format-justify-left-symbolic")
        .tooltip_text("Lirik")
        .css_classes(["lyrics-toggle"])
        .valign(gtk::Align::Center)
        .build();
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    head.append(&art);
    head.append(&text);
    head.append(&lyrics_btn);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&prev);
    buttons.append(&play);
    buttons.append(&next);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_top(10);
    // Idle extras under the big clock: system stats and recent clipboard items.
    let stats = gtk::Label::builder().css_classes(["stats"]).build();
    let clips = gtk::Box::new(gtk::Orientation::Vertical, 4);
    body.append(&head);
    body.append(&stats);
    body.append(&clips);
    body.append(&seek_row);
    body.append(&buttons);
    // Full lyrics sit to the right of the player when the lyrics button is on.
    let lyrics_list = gtk::ListBox::new();
    lyrics_list.set_selection_mode(gtk::SelectionMode::None);
    let lyrics_scroll = gtk::ScrolledWindow::builder()
        .child(&lyrics_list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        // min-content-width is ignored without a horizontal scrollbar, so size it directly.
        .width_request(260)
        .height_request(150)
        .margin_top(10)
        .visible(false)
        .build();
    let split = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    split.append(&body);
    split.append(&lyrics_scroll);
    let expanded = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(200)
        .child(&split)
        .build();

    let notch = gtk::Box::new(gtk::Orientation::Vertical, 2);
    notch.add_css_class("notch");
    notch.append(&top);
    notch.append(&lyric);
    notch.append(&lyric_rom);
    notch.append(&expanded);
    window.set_child(Some(&notch));

    let ui = Ui {
        notch: notch.clone(),
        seek,
        time,
        compact,
        mini_art,
        art,
        eq,
        lyric,
        expanded,
        title,
        artist,
        play,
        buttons,
        mascot,
        lyrics_btn,
        lyrics_list,
        lyrics_scroll,
        lyric_rom,
        mic,
        stats,
        clips,
        accent: gtk::CssProvider::new(),
    };
    // Cover-tinted gradient lives in its own provider so it can be swapped per track.
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &ui.accent,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    let state: Shared = Rc::new(RefCell::new(State {
        minimal: minimal_flag().exists(),
        mascot_off: no_mascot_flag().exists(),
        ..Default::default()
    }));

    let s = state.clone();
    ui.mascot.set_draw_func(move |_, cr, w, h| {
        let st = s.borrow();
        draw_eyes(cr, w as f64, h as f64, st.mood, st.blink);
    });
    let (u, s) = (ui.clone(), state.clone());
    ui.lyrics_btn.connect_toggled(move |_| refresh_compact(&u, &s.borrow()));
    let s = state.clone();
    ui.lyrics_list.connect_row_activated(move |_, row| {
        let ms = s.borrow().lyrics.get(row.index() as usize).map(|l| l.0);
        if let Some(ms) = ms {
            seek_to(&s, ms as f64 / 1000.0);
        }
    });
    spawn_idle_watch(&ui, &state);
    spawn_mic_watch(&ui, &state);

    // Gestures on the notch: wheel/two-finger up-down = volume, left-right swipe = next/previous,
    // middle click = play/pause. Inner scrollables (lyrics, seek bar) handle their own scroll first.
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    let acc = Rc::new(std::cell::Cell::new((0.0f64, 0.0f64)));
    let (u, s, a) = (ui.clone(), state.clone(), acc.clone());
    scroll.connect_scroll(move |_, dx, dy| {
        let acc = &a;
        let (mut ax, mut ay) = acc.get();
        ax += dx;
        ay += dy;
        if ay.abs() >= 1.0 {
            change_volume(&u, &s, if ay < 0.0 { "5%+" } else { "5%-" });
            ay = 0.0;
            ax = 0.0;
        } else if ax.abs() >= 4.0 {
            let p = s.borrow().player.clone();
            if let Some(p) = p {
                let next = ax > 0.0;
                glib::spawn_future_local(async move {
                    let _ = if next { p.next().await } else { p.previous().await };
                });
            }
            ax = 0.0;
            ay = 0.0;
        }
        acc.set((ax, ay));
        glib::Propagation::Stop
    });
    // A finished touchpad gesture shouldn't leave half a step pending for the next one.
    scroll.connect_scroll_end(move |_| acc.set((0.0, 0.0)));
    notch.add_controller(scroll);
    let middle = gtk::GestureClick::builder().button(2).build();
    let s = state.clone();
    middle.connect_pressed(move |_, _, _, _| {
        let p = s.borrow().player.clone();
        if let Some(p) = p {
            glib::spawn_future_local(async move {
                let _ = p.play_pause().await;
            });
        }
    });
    notch.add_controller(middle);

    // Hover opens, leaving closes.
    let hover = gtk::EventControllerMotion::new();
    let (u1, s1, u2, s2) = (ui.clone(), state.clone(), ui.clone(), state.clone());
    hover.connect_enter(move |_, _, _| {
        u1.expanded.set_reveal_child(true);
        refresh_compact(&u1, &s1.borrow());
    });
    hover.connect_leave(move |_| {
        u2.expanded.set_reveal_child(false);
        refresh_compact(&u2, &s2.borrow());
    });
    notch.add_controller(hover);

    // While the mouse button is held, only move the slider; seek once on release so the
    // player doesn't stutter through every drag step. Clicks, scroll and keys seek right away.
    // A legacy controller sees raw press/release even though the scale's own drag gesture claims them.
    let pressed = Rc::new(std::cell::Cell::new(false));
    let raw = gtk::EventControllerLegacy::new();
    raw.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (p, s) = (pressed.clone(), state.clone());
    raw.connect_event(move |_, ev| {
        match ev.event_type() {
            gtk::gdk::EventType::ButtonPress => p.set(true),
            gtk::gdk::EventType::ButtonRelease => {
                p.set(false);
                let target = s.borrow_mut().seeking.take();
                if let Some(secs) = target {
                    seek_to(&s, secs);
                }
            }
            _ => {}
        }
        glib::Propagation::Proceed
    });
    ui.seek.add_controller(raw);
    // change-value only fires on user input, not on our own set_value() from polling.
    let (u, s) = (ui.clone(), state.clone());
    ui.seek.connect_change_value(move |_, _, secs| {
        if pressed.get() {
            s.borrow_mut().seeking = Some(secs);
            u.time.set_label(&format!("{} / {}", mmss(secs as i64), mmss(u.seek.adjustment().upper() as i64)));
        } else {
            seek_to(&s, secs);
        }
        glib::Propagation::Proceed
    });

    spawn_tray(app, &ui, &state);

    for (btn, action) in [(&prev, "prev"), (&ui.play, "play"), (&next, "next")] {
        let state = state.clone();
        btn.connect_clicked(move |_| {
            let Some(p) = state.borrow().player.clone() else { return };
            glib::spawn_future_local(async move {
                let _ = match action {
                    "prev" => p.previous().await,
                    "next" => p.next().await,
                    _ => p.play_pause().await,
                };
            });
        });
    }

    refresh_compact(&ui, &state.borrow());
    let (u, s) = (ui.clone(), state.clone());
    glib::timeout_add_seconds_local(1, move || {
        refresh_compact(&u, &s.borrow());
        if u.stats.is_visible() && u.expanded.reveals_child() {
            let text = read_stats(&mut s.borrow_mut().cpu_prev);
            u.stats.set_label(&text);
        }
        glib::ControlFlow::Continue
    });

    // ponytail: MPRIS never signals Position, so poll it while lyrics or the seek bar are on screen; extrapolate locally if 2 calls/s ever matters.
    let (u, s) = (ui.clone(), state.clone());
    glib::timeout_add_local(Duration::from_millis(500), move || {
        let st = s.borrow();
        let visible = !st.lyrics.is_empty() || u.expanded.reveals_child();
        drop(st);
        let st = s.borrow();
        if let (true, true, None, Some(p)) = (st.playing, visible, st.seeking, st.player.clone()) {
            let (u, s) = (u.clone(), s.clone());
            glib::spawn_future_local(async move {
                if let Ok(us) = p.position().await {
                    let line = current_line(&s.borrow().lyrics, us / 1000).unwrap_or_default();
                    u.lyric.set_label(&line);
                    let idx = current_index(&s.borrow().lyrics, us / 1000);
                    let rom = idx.and_then(|i| s.borrow().lyrics_rom.get(i).cloned().flatten());
                    u.lyric_rom.set_label(rom.as_deref().unwrap_or_default());
                    highlight_lyric(&u, &mut s.borrow_mut(), idx);
                    u.seek.set_value(us as f64 / 1e6);
                    u.time.set_label(&format!("{} / {}", mmss(us / 1_000_000), mmss(u.seek.adjustment().upper() as i64)));
                }
            });
        }
        glib::ControlFlow::Continue
    });

    let (u, s) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        if let Err(e) = mpris_loop(u, s).await {
            eprintln!("mpris: {e}");
        }
    });
    let (u, s) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        if let Err(e) = battery_loop(u, s).await {
            eprintln!("battery: {e}");
        }
    });
    let (u, s) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        if let Err(e) = network_loop(u, s).await {
            eprintln!("network: {e}");
        }
    });
    let (u, s) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        if let Err(e) = bluetooth_loop(u, s).await {
            eprintln!("bluetooth: {e}");
        }
    });
    let (u, s) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        if let Err(e) = clipboard_loop(u, s).await {
            eprintln!("clipboard: {e}");
        }
    });
    let (u, s) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        if let Err(e) = phone_loop(u, s).await {
            eprintln!("kdeconnect: {e}");
        }
    });

    window.present();
}

fn refresh_compact(ui: &Ui, st: &State) {
    let text = if let Some(a) = &st.alert {
        a.clone()
    } else if st.playing {
        format!("♪  {}", st.track)
    } else {
        glib::DateTime::now_local().and_then(|d| d.format("%H:%M")).map(|s| s.to_string()).unwrap_or_default()
    };
    ui.compact.set_label(&text);
    // Minimal mode: closed and quiet, the notch is just a thin black line. Lyrics and alerts still show.
    let open = ui.expanded.reveals_child();
    // A player that is open but stopped (no title) counts as idle, same as no player at all.
    let has_track = st.player.is_some() && !st.track.is_empty();
    let has_lyrics = has_track && !st.lyrics.is_empty();
    let full_lyrics = open && has_lyrics && ui.lyrics_btn.is_active();
    ui.lyrics_btn.set_visible(has_lyrics);
    ui.lyrics_scroll.set_visible(full_lyrics);
    ui.lyric.set_visible(st.playing && !st.lyrics.is_empty() && st.alert.is_none() && !full_lyrics);
    ui.lyric_rom.set_visible(ui.lyric.is_visible() && st.lyrics_rom.iter().any(Option::is_some));
    ui.stats.set_visible(!has_track);
    ui.clips.set_visible(!has_track && ui.clips.first_child().is_some());
    if has_track && ui.art.paintable().is_some() {
        ui.notch.add_css_class("tinted");
    } else {
        ui.notch.remove_css_class("tinted");
    }
    // When open, the expanded view shows either the track or a big clock, so the top row would only repeat it.
    let show_compact = st.alert.is_some() || (!open && !st.minimal);
    ui.mic.set_visible(!st.mic_apps.is_empty() && show_compact);
    // The mascot stands in for the clock: closed, quiet and no music.
    let mascot = st.mood != Mood::None && show_compact && st.alert.is_none() && !st.playing;
    ui.mascot.set_visible(mascot);
    ui.compact.set_visible(show_compact && !mascot);
    let media = show_compact && st.playing && st.alert.is_none();
    ui.eq.set_visible(media);
    ui.mini_art.set_visible(media && ui.mini_art.paintable().is_some());
    if ui.expanded.reveals_child() {
        ui.notch.add_css_class("open");
    } else {
        ui.notch.remove_css_class("open");
    }
    if !open && !show_compact && !ui.lyric.is_visible() {
        ui.notch.add_css_class("thin");
    } else {
        ui.notch.remove_css_class("thin");
    }
    // Idle: the expanded view is a big centered clock with the date under it.
    ui.buttons.set_visible(has_track);
    let xalign = if has_track { 0.0 } else { 0.5 };
    ui.title.set_xalign(xalign);
    ui.artist.set_xalign(xalign);
    if has_track {
        ui.title.remove_css_class("clock");
    } else {
        let now = glib::DateTime::now_local().ok();
        ui.title.add_css_class("clock");
        ui.title.set_label(&now.as_ref().and_then(|d| d.format("%H:%M").ok()).unwrap_or_default());
        ui.artist.set_label(&now.as_ref().map(tanggal).unwrap_or_default());
        ui.art.set_visible(false);
        ui.seek.parent().unwrap().set_visible(false);
    }
}

fn show_alert(ui: &Ui, state: &Shared, text: String) {
    show_alert_for(ui, state, text, 3);
}

fn show_alert_for(ui: &Ui, state: &Shared, text: String, secs: u32) {
    let seq = {
        let mut st = state.borrow_mut();
        st.alert = Some(text);
        st.alert_seq += 1;
        st.alert_seq
    };
    refresh_compact(ui, &state.borrow());
    let (u, s) = (ui.clone(), state.clone());
    glib::timeout_add_seconds_local_once(secs, move || {
        if s.borrow().alert_seq == seq {
            s.borrow_mut().alert = None;
            refresh_compact(&u, &s.borrow());
        }
    });
}

async fn mpris_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.DBus.Properties")
        .member("PropertiesChanged")
        .path("/org/mpris/MediaPlayer2")
        .build()?;
    let props = MessageStream::for_match_rule(rule, &conn, None).await?.map(|_| ());
    let owners = dbus
        .receive_name_owner_changed()
        .await?
        .filter(|s| std::future::ready(s.args().is_ok_and(|a| a.name().starts_with("org.mpris.MediaPlayer2."))))
        .map(|_| ());
    let mut events = futures_util::stream::select(props, owners);

    // Re-pick from every live player on each event instead of tracking incrementally,
    // so closed players and failed calls can never leave a stale choice behind.
    loop {
        let current = state.borrow().player.as_ref().map(|p| p.inner().destination().to_string());
        let picked = pick_player(&conn, &dbus, current.as_deref()).await;
        if picked.is_none() {
            clear_player(&ui, &state);
        } else {
            state.borrow_mut().player = picked;
            refresh_player(&ui, &state).await;
        }
        if events.next().await.is_none() {
            return Ok(());
        }
    }
}

/// Current player if still playing, else any playing one, else current if alive, else any.
async fn pick_player(conn: &Connection, dbus: &zbus::fdo::DBusProxy<'_>, current: Option<&str>) -> Option<PlayerProxy<'static>> {
    let mut alive = vec![];
    for name in dbus.list_names().await.ok()? {
        if !name.starts_with("org.mpris.MediaPlayer2.") {
            continue;
        }
        let Ok(owner) = dbus.get_name_owner(name.as_ref()).await else { continue };
        let Ok(p) = player(conn, owner.to_string()).await else { continue };
        let Ok(status) = p.playback_status().await else { continue };
        alive.push((owner.to_string(), status == "Playing", p));
    }
    let is_current = |o: &str| Some(o) == current;
    let idx = alive
        .iter()
        .position(|(o, playing, _)| *playing && is_current(o))
        .or_else(|| alive.iter().position(|(_, playing, _)| *playing))
        .or_else(|| alive.iter().position(|(o, _, _)| is_current(o)))
        .or((!alive.is_empty()).then_some(0))?;
    Some(alive.swap_remove(idx).2)
}

/// Forget the current player and its track, leaving everything else (alerts, mic, settings, mascot) alone.
fn clear_player(ui: &Ui, state: &Shared) {
    {
        let mut st = state.borrow_mut();
        st.player = None;
        st.playing = false;
        st.track.clear();
        st.track_id = None;
        st.art_url.clear();
        st.seeking = None;
        st.lyrics.clear();
        st.lyrics_rom.clear();
        st.lyric_idx = None;
    }
    ui.lyrics_list.remove_all();
    refresh_compact(ui, &state.borrow());
}

async fn player(conn: &Connection, dest: String) -> zbus::Result<PlayerProxy<'static>> {
    PlayerProxy::builder(conn)
        .destination(dest)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
}

async fn refresh_player(ui: &Ui, state: &Shared) {
    let Some(p) = state.borrow().player.clone() else { return };
    let (Ok(meta), Ok(status)) = (p.metadata().await, p.playback_status().await) else {
        // Player went away.
        clear_player(ui, state);
        return;
    };
    let str_of = |k: &str| meta.get(k).and_then(|v| String::try_from(v.try_clone().ok()?).ok()).unwrap_or_default();
    let title = str_of("xesam:title");
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok())
        .map(|a| a.join(", "))
        .unwrap_or_default();
    let length_s = meta
        .get("mpris:length")
        .and_then(|v| i64::try_from(v.try_clone().ok()?).ok().or_else(|| u64::try_from(v.try_clone().ok()?).ok().map(|n| n as i64)))
        .unwrap_or(0)
        / 1_000_000;

    let playing = status == "Playing";
    let art_url = str_of("mpris:artUrl");
    if state.borrow().art_url != art_url {
        state.borrow_mut().art_url = art_url.clone();
        let (ui, state) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            let url = art_url.clone();
            let cover = gio::spawn_blocking(move || square_cover(&load_art(&url)?)).await.ok().flatten();
            if let Some((_, _, _, (r, g, b))) = &cover {
                ui.accent.load_from_string(&format!(
                    ".notch.tinted {{ background: linear-gradient(160deg, #020510 0%, rgb({},{},{}) 65%, rgb({r},{g},{b}) 100%); }}",
                    r / 3,
                    g / 3,
                    b / 3
                ));
            }
            if state.borrow().art_url != art_url {
                return; // track changed while loading
            }
            let tex = cover.map(|(pixels, alpha, stride, _)| {
                let format = if alpha { gtk::gdk::MemoryFormat::R8g8b8a8 } else { gtk::gdk::MemoryFormat::R8g8b8 };
                gtk::gdk::MemoryTexture::new(112, 112, format, &pixels, stride).upcast::<gtk::gdk::Texture>()
            });
            ui.art.set_paintable(tex.as_ref());
            ui.mini_art.set_paintable(tex.as_ref());
            ui.art.set_visible(tex.is_some());
            refresh_compact(&ui, &state.borrow());
        });
    }
    state.borrow_mut().track_id = meta.get("mpris:trackid").and_then(|v| OwnedObjectPath::try_from(v.try_clone().ok()?).ok());
    ui.seek.set_range(0.0, length_s.max(1) as f64);
    ui.seek.parent().unwrap().set_visible(length_s > 0);
    ui.title.set_label(&title);
    ui.artist.set_label(&artist);
    ui.play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });

    let changed = {
        let mut st = state.borrow_mut();
        st.playing = playing;
        let changed = st.track != title;
        if changed {
            st.track = title.clone();
            st.lyrics.clear();
            st.lyrics_rom.clear();
            st.lyric_idx = None;
            ui.lyric.set_label("");
            ui.lyrics_list.remove_all();
        }
        changed
    };
    refresh_compact(ui, &state.borrow());

    if changed && !title.is_empty() {
        let (ui, state) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            let t = title.clone();
            let lyrics = gio::spawn_blocking(move || fetch_lyrics(&t, &artist, length_s)).await.unwrap_or_default();
            if state.borrow().track == title {
                let rom = romanize_song(&lyrics);
                for ((_, line), r) in lyrics.iter().zip(&rom) {
                    let text = if line.is_empty() { "♪".to_string() } else { glib::markup_escape_text(line).to_string() };
                    let markup = match r {
                        Some(r) => format!("{text}\n<span size='small' alpha='60%'>{}</span>", glib::markup_escape_text(r)),
                        None => text,
                    };
                    let label = gtk::Label::builder()
                        .label(markup)
                        .use_markup(true)
                        .css_classes(["lrc"])
                        .xalign(0.0)
                        .wrap(true)
                        .max_width_chars(30)
                        .build();
                    ui.lyrics_list.append(&label);
                }
                state.borrow_mut().lyrics = lyrics;
                state.borrow_mut().lyrics_rom = rom;
                refresh_compact(&ui, &state.borrow());
            }
        });
    }
}

async fn battery_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::system().await?;
    let bat = BatteryProxy::new(&conn).await?;
    if !bat.is_present().await? {
        return Ok(());
    }
    let mut changes = bat.receive_state_changed().await;
    changes.next().await; // first item is the current value, not a change
    while let Some(c) = changes.next().await {
        let pct = bat.percentage().await.unwrap_or(0.0).round();
        // UPower states: 1 charging, 2 discharging, 4 fully charged.
        let text = match c.get().await? {
            1 => format!("⚡ Charging  {pct}%"),
            2 => format!("🔋 On battery  {pct}%"),
            4 => "⚡ Fully charged".into(),
            _ => continue,
        };
        show_alert(&ui, &state, text);
    }
    Ok(())
}

async fn network_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::system().await?;
    let nm = NetworkProxy::new(&conn).await?;
    let mut changes = nm.receive_state_changed().await;
    // NM states: 70 = connected globally, <= 20 = disconnected. Ignore the steps in between
    // so one reconnect shows one alert, not five.
    let mut online = changes.next().await.is_some() && nm.state().await? >= 70;
    while let Some(c) = changes.next().await {
        let now = c.get().await?;
        if now >= 70 && !online {
            online = true;
            let path = nm.primary_connection().await?;
            let active = ActiveConnectionProxy::builder(&conn).path(path).build().await?;
            let name = active.id().await.unwrap_or_default();
            let icon = if active.kind().await.as_deref() == Ok("802-11-wireless") { "📶" } else { "🔌" };
            show_alert(&ui, &state, format!("{icon}  Terhubung · {name}"));
        } else if now <= 20 && online {
            online = false;
            show_alert(&ui, &state, "⚠  Internet terputus".into());
        }
    }
    Ok(())
}

async fn bluetooth_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::system().await?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.bluez")
        .interface("org.freedesktop.DBus.Properties")
        .member("PropertiesChanged")
        .arg(0, "org.bluez.Device1")
        .build()?;
    let mut stream = MessageStream::for_match_rule(rule, &conn, None).await?;
    while let Some(Ok(msg)) = stream.next().await {
        let Ok((_, changed, _)) = msg.body().deserialize::<(String, HashMap<String, OwnedVariant>, Vec<String>)>() else { continue };
        let Some(connected) = changed.get("Connected").and_then(|v| bool::try_from(v.try_clone().ok()?).ok()) else { continue };
        let Some(path) = msg.header().path().map(|p| p.to_owned()) else { continue };
        let dev = BtDeviceProxy::builder(&conn).path(path.clone()).build().await?;
        // Only audio gear (headphones, earbuds, speakers); mice and keyboards connect too often to be worth a popup.
        if !dev.icon().await.unwrap_or_default().starts_with("audio") {
            continue;
        }
        let name = dev.alias().await.unwrap_or_default();
        if !connected {
            show_alert(&ui, &state, format!("🎧  {name} terputus"));
            continue;
        }
        show_alert(&ui, &state, format!("🎧  {name}"));
        // BlueZ publishes the battery a moment after connecting, if the device reports one.
        let (ui, state, conn) = (ui.clone(), state.clone(), conn.clone());
        glib::spawn_future_local(async move {
            glib::timeout_future_seconds(2).await;
            let Ok(bat) = BtBatteryProxy::builder(&conn).path(path).build().await else { return };
            if let Ok(pct) = bat.percentage().await {
                show_alert(&ui, &state, format!("🎧  {name} · {pct}%"));
            }
        });
    }
    Ok(())
}

/// Light up line `idx` in the full lyrics list and scroll it to the middle.
fn highlight_lyric(ui: &Ui, st: &mut State, idx: Option<usize>) {
    if st.lyric_idx == idx {
        return;
    }
    let row = |i: Option<usize>| i.and_then(|i| ui.lyrics_list.row_at_index(i as i32)).and_then(|r| r.child());
    if let Some(old) = row(st.lyric_idx) {
        old.remove_css_class("now");
    }
    if let Some(new) = row(idx) {
        new.add_css_class("now");
        if let Some(b) = new.compute_bounds(&ui.lyrics_list) {
            let adj = ui.lyrics_scroll.vadjustment();
            adj.set_value((b.y() as f64 + b.height() as f64 / 2.0 - adj.page_size() / 2.0).max(0.0));
        }
    }
    st.lyric_idx = idx;
}

const HARI: [&str; 7] = ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"];
const BULAN: [&str; 12] =
    ["Januari", "Februari", "Maret", "April", "Mei", "Juni", "Juli", "Agustus", "September", "Oktober", "November", "Desember"];

/// "Senin, 26 Oktober 2026", independent of the system locale.
fn tanggal(d: &glib::DateTime) -> String {
    let (day, month) = (HARI[d.day_of_week() as usize - 1], BULAN[d.month() as usize - 1]);
    format!("{day}, {} {month} {}", d.day_of_month(), d.year())
}

/// Center-crop a cover to a square and shrink it to 112px, 2x the largest size we draw.
/// Returns raw pixels (Pixbuf itself can't cross threads): (bytes, has_alpha, rowstride, accent rgb).
fn square_cover(bytes: &[u8]) -> Option<(glib::Bytes, bool, usize, (u8, u8, u8))> {
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    let pb = loader.pixbuf()?;
    let side = pb.width().min(pb.height());
    let sq = pb
        .new_subpixbuf((pb.width() - side) / 2, (pb.height() - side) / 2, side, side)
        .scale_simple(112, 112, gtk::gdk_pixbuf::InterpType::Bilinear)?;
    let (pixels, alpha, stride) = (sq.read_pixel_bytes(), sq.has_alpha(), sq.rowstride() as usize);
    let accent = accent_color(&pixels, if alpha { 4 } else { 3 }, stride, 112);
    Some((pixels, alpha, stride, accent))
}

/// A dark, saturated version of the cover's main color, for the notch gradient.
/// Colorful pixels count more than gray ones so a mostly-white cover still gives a hue.
fn accent_color(px: &[u8], channels: usize, stride: usize, size: usize) -> (u8, u8, u8) {
    let (mut r, mut g, mut b, mut w) = (0.0, 0.0, 0.0, 0.0);
    for y in 0..size {
        for x in 0..size {
            let i = y * stride + x * channels;
            let (pr, pg, pb) = (px[i] as f64, px[i + 1] as f64, px[i + 2] as f64);
            let sat = pr.max(pg).max(pb) - pr.min(pg).min(pb) + 1.0;
            r += pr * sat;
            g += pg * sat;
            b += pb * sat;
            w += sat;
        }
    }
    let (r, g, b) = (r / w, g / w, b / w);
    // Scale so the brightest channel lands at ~110 (dark enough for white text), keeping the hue.
    let k = 110.0 / r.max(g).max(b).max(1.0);
    ((r * k) as u8, (g * k) as u8, (b * k) as u8)
}

fn draw_eyes(cr: &gtk::cairo::Context, w: f64, h: f64, mood: Mood, blink: bool) {
    use std::f64::consts::PI;
    let (ew, eh, gap, cy) = (9.0, 13.0, 8.0, h / 2.0);
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.set_line_width(2.0);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    for x in [w / 2.0 - gap / 2.0 - ew, w / 2.0 + gap / 2.0] {
        match mood {
            Mood::Awake if !blink => {
                // Upright pill.
                let r = ew / 2.0;
                cr.new_sub_path();
                cr.arc(x + r, cy - eh / 2.0 + r, r, PI, 0.0);
                cr.arc(x + r, cy + eh / 2.0 - r, r, 0.0, PI);
                cr.close_path();
                let _ = cr.fill();
            }
            Mood::Awake | Mood::Sleep => {
                // Closed: a soft downward curve.
                cr.move_to(x, cy);
                cr.curve_to(x + ew * 0.3, cy + 3.0, x + ew * 0.7, cy + 3.0, x + ew, cy);
                let _ = cr.stroke();
            }
            Mood::Wave => {
                // Happy: ^ ^
                cr.move_to(x, cy + 3.0);
                cr.line_to(x + ew / 2.0, cy - 3.0);
                cr.line_to(x + ew, cy + 3.0);
                let _ = cr.stroke();
            }
            Mood::None => {}
        }
    }
    if mood == Mood::Sleep {
        cr.set_font_size(9.0);
        cr.move_to(w / 2.0 + gap / 2.0 + ew + 3.0, cy - 2.0);
        let _ = cr.show_text("z");
    }
}

/// Idle levels from the compositor: 1 = quiet for 30 s, 2 = quiet for 3 min.
struct IdleWatch(async_channel::Sender<(u8, bool)>);

impl wayland_client::Dispatch<wayland_client::protocol::wl_registry::WlRegistry, wayland_client::globals::GlobalListContents> for IdleWatch {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_registry::WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &wayland_client::globals::GlobalListContents,
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(IdleWatch: ignore wayland_client::protocol::wl_seat::WlSeat);
wayland_client::delegate_noop!(IdleWatch: wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1);

impl wayland_client::Dispatch<wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::ExtIdleNotificationV1, u8> for IdleWatch {
    fn event(
        w: &mut Self,
        _: &wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::ExtIdleNotificationV1,
        event: wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::Event,
        level: &u8,
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
        use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::Event;
        let idle = matches!(event, Event::Idled);
        let _ = w.0.send_blocking((*level, idle));
    }
}

/// The compositor tells us when the user goes quiet and comes back (ext-idle-notify), so no polling.
/// It runs on its own Wayland connection in a thread, separate from GTK's.
fn spawn_idle_watch(ui: &Ui, state: &Shared) {
    let (tx, rx) = async_channel::unbounded();
    std::thread::spawn(move || {
        let run = || -> Result<(), Box<dyn std::error::Error>> {
            use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;
            let conn = wayland_client::Connection::connect_to_env()?;
            let (globals, mut queue) = wayland_client::globals::registry_queue_init::<IdleWatch>(&conn)?;
            let qh = queue.handle();
            let seat: wayland_client::protocol::wl_seat::WlSeat = globals.bind(&qh, 1..=1, ())?;
            let notifier: ExtIdleNotifierV1 = globals.bind(&qh, 1..=1, ())?;
            notifier.get_idle_notification(30_000, &seat, &qh, 1u8);
            notifier.get_idle_notification(180_000, &seat, &qh, 2u8);
            let mut watch = IdleWatch(tx);
            loop {
                queue.blocking_dispatch(&mut watch)?;
            }
        };
        if let Err(e) = run() {
            eprintln!("idle: {e}");
        }
    });
    let (ui, state) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        while let Ok((level, idle)) = rx.recv().await {
            let mood = state.borrow().mood;
            let next = match (level, idle) {
                _ if state.borrow().mascot_off => Mood::None,
                (1, true) if mood == Mood::None => Mood::Awake,
                (2, true) => Mood::Sleep,
                (_, false) if matches!(mood, Mood::Awake | Mood::Sleep) => Mood::Wave,
                _ => mood,
            };
            set_mood(&ui, &state, next);
        }
    });
}

fn set_mood(ui: &Ui, state: &Shared, mood: Mood) {
    if state.borrow().mood == mood {
        return;
    }
    state.borrow_mut().mood = mood;
    refresh_compact(ui, &state.borrow());
    ui.mascot.queue_draw();
    let (u, s) = (ui.clone(), state.clone());
    match mood {
        // Blink every few seconds while awake; the timer stops itself once the mood changes.
        Mood::Awake => {
            glib::timeout_add_local(Duration::from_millis(3500), move || {
                if s.borrow().mood != Mood::Awake {
                    return glib::ControlFlow::Break;
                }
                s.borrow_mut().blink = true;
                u.mascot.queue_draw();
                let (u, s) = (u.clone(), s.clone());
                glib::timeout_add_local_once(Duration::from_millis(140), move || {
                    s.borrow_mut().blink = false;
                    u.mascot.queue_draw();
                });
                glib::ControlFlow::Continue
            });
        }
        // Say hi for two seconds, then hand the notch back to the clock.
        Mood::Wave => {
            glib::timeout_add_seconds_local_once(2, move || {
                if s.borrow().mood == Mood::Wave {
                    set_mood(&u, &s, Mood::None);
                }
            });
        }
        _ => {}
    }
}

fn change_volume(ui: &Ui, state: &Shared, step: &'static str) {
    let (ui, state) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        let out = gio::spawn_blocking(move || {
            let sink = "@DEFAULT_AUDIO_SINK@";
            std::process::Command::new("wpctl").args(["set-volume", "-l", "1.0", sink, step]).status().ok()?;
            let out = std::process::Command::new("wpctl").args(["get-volume", sink]).output().ok()?;
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        })
        .await
        .ok()
        .flatten();
        // "Volume: 0.45" or "Volume: 0.45 [MUTED]"
        if let Some(v) = out.and_then(|o| o.split_whitespace().nth(1)?.parse::<f64>().ok()) {
            show_alert_for(&ui, &state, format!("🔊  {}%", (v * 100.0).round()), 1);
        }
    });
}

/// "CPU 12% · RAM 6.1/15.4 GB · 52°C" from /proc and the CPU's hwmon sensor.
fn read_stats(cpu_prev: &mut Option<(u64, u64)>) -> String {
    let mut parts = vec![];
    if let Some(line) = std::fs::read_to_string("/proc/stat").ok().and_then(|s| s.lines().next().map(str::to_owned)) {
        let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect();
        let total: u64 = v.iter().sum();
        let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
        let busy = total - idle;
        if let Some((pb, pt)) = cpu_prev.replace((busy, total)) {
            if total > pt {
                parts.push(format!("CPU {}%", (busy - pb) * 100 / (total - pt)));
            }
        }
    }
    if let Ok(m) = std::fs::read_to_string("/proc/meminfo") {
        let kb = |key: &str| -> f64 {
            m.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()).unwrap_or(0.0)
        };
        let (total, avail) = (kb("MemTotal:"), kb("MemAvailable:"));
        parts.push(format!("RAM {:.1}/{:.1} GB", (total - avail) / 1048576.0, total / 1048576.0));
    }
    if let Some(t) = cpu_temp() {
        parts.push(format!("{t}°C"));
    }
    parts.join("  ·  ")
}

fn cpu_temp() -> Option<i64> {
    for dir in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let name = std::fs::read_to_string(dir.path().join("name")).unwrap_or_default();
        if ["k10temp", "coretemp", "zenpower", "cpu_thermal"].contains(&name.trim()) {
            let milli: i64 = std::fs::read_to_string(dir.path().join("temp1_input")).ok()?.trim().parse().ok()?;
            return Some(milli / 1000);
        }
    }
    None
}

/// Last 3 clipboard items from Klipper; clicking one makes it the current clipboard again.
async fn clipboard_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let klipper = KlipperProxy::new(&conn).await?;
    let mut updates = klipper.receive_clipboard_history_updated().await?;
    loop {
        while let Some(c) = ui.clips.first_child() {
            ui.clips.remove(&c);
        }
        let history = klipper.get_clipboard_history_menu().await.inspect_err(|e| eprintln!("clipboard: {e}")).unwrap_or_default();
        for item in history.into_iter().take(3) {
            let flat: String = item.split_whitespace().collect::<Vec<_>>().join(" ");
            if flat.is_empty() {
                continue;
            }
            let short: String = if flat.chars().count() > 40 { flat.chars().take(40).collect::<String>() + "…" } else { flat };
            let btn = gtk::Button::builder().label(format!("📋  {short}")).css_classes(["clip"]).build();
            btn.child().and_then(|c| c.downcast::<gtk::Label>().ok()).inspect(|l| l.set_xalign(0.0));
            let k = klipper.clone();
            btn.connect_clicked(move |_| {
                let (k, item) = (k.clone(), item.clone());
                glib::spawn_future_local(async move {
                    let _ = k.set_clipboard_contents(&item).await;
                });
            });
            ui.clips.append(&btn);
        }
        refresh_compact(&ui, &state.borrow());
        if updates.next().await.is_none() {
            return Ok(());
        }
    }
}

/// Phone notifications forwarded by KDE Connect (calls show up as the dialer's notification).
async fn phone_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.kde.kdeconnect.device.notifications")
        .member("notificationPosted")
        .build()?;
    let mut stream = MessageStream::for_match_rule(rule, &conn, None).await?;
    while let Some(Ok(msg)) = stream.next().await {
        let Ok(id) = msg.body().deserialize::<String>() else { continue };
        let Some(path) = msg.header().path().map(|p| format!("{p}/{id}")) else { continue };
        let Ok(n) = PhoneNotificationProxy::builder(&conn).path(path).build().await else { continue };
        if n.silent().await.unwrap_or(false) {
            continue;
        }
        let (app, title, text) = (n.app_name().await.unwrap_or_default(), n.title().await.unwrap_or_default(), n.text().await.unwrap_or_default());
        let body = [title, text].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(": ");
        show_alert_for(&ui, &state, format!("📱  {app} · {body}"), 5);
    }
    Ok(())
}

/// Watches PipeWire through `pactl subscribe` and reports which apps record from a mic.
/// ponytail: shells out to pactl instead of linking libpulse/pipewire; swap if pactl ever goes missing.
fn spawn_mic_watch(ui: &Ui, state: &Shared) {
    let (tx, rx) = async_channel::unbounded::<Vec<String>>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let recording = || -> Vec<String> {
            let Ok(out) = std::process::Command::new("pactl").args(["-f", "json", "list", "source-outputs"]).output() else { return vec![] };
            let list: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
            let mut apps: Vec<String> = list
                .as_array()
                .into_iter()
                .flatten()
                .filter(|o| !o["corked"].as_bool().unwrap_or(false))
                // Level meters (volume applets) open monitor streams; they are not a mic in use.
                .filter(|o| o["properties"]["stream.monitor"].as_str() != Some("true"))
                .filter(|o| !o["properties"]["media.name"].as_str().unwrap_or("").contains("Peak"))
                .filter_map(|o| o["properties"]["application.name"].as_str().map(str::to_owned))
                .collect();
            apps.dedup();
            apps
        };
        let mut last = recording();
        let _ = tx.send_blocking(last.clone());
        let mut cmd = std::process::Command::new("pactl");
        cmd.arg("subscribe").stdout(std::process::Stdio::piped());
        // Have the kernel stop pactl when this thread (and so Nusa) dies, instead of leaving it orphaned.
        // SAFETY: prctl is async-signal-safe, which is all pre_exec requires.
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut cmd, || {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let Ok(mut child) = cmd.spawn() else { return };
        for line in std::io::BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
            if line.contains("source-output") {
                let now = recording();
                if now != last {
                    last = now;
                    let _ = tx.send_blocking(last.clone());
                }
            }
        }
    });
    let (ui, state) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        while let Ok(apps) = rx.recv().await {
            let started = state.borrow().mic_apps.is_empty() && !apps.is_empty();
            let name = apps.join(", ");
            state.borrow_mut().mic_apps = apps;
            if started {
                show_alert(&ui, &state, format!("🎙  Mic dipakai · {name}"));
            } else {
                refresh_compact(&ui, &state.borrow());
            }
        }
    });
}

/// Romanize every line with one script decision for the whole song, so a Japanese song's
/// kanji-only lines aren't read as Mandarin. Japanese itself is left as-is for now.
fn romanize_song(lyrics: &[(i64, String)]) -> Vec<Option<String>> {
    let has_kana = lyrics.iter().any(|(_, l)| l.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)));
    lyrics
        .iter()
        .map(|(_, l)| {
            let r = if l.chars().any(is_hangul) {
                romanize_korean(l)
            } else if has_kana {
                return None;
            } else if l.chars().any(is_han) {
                romanize_chinese(l)
            } else if l.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)) {
                romanize_cyrillic(l)
            } else {
                return None;
            };
            (r != *l).then_some(r)
        })
        .collect()
}

fn is_hangul(c: char) -> bool {
    ('\u{ac00}'..='\u{d7a3}').contains(&c)
}

fn is_han(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c)
}

/// Revised Romanization, plus the two sound rules that matter most when singing along:
/// a final consonant carries over to a following silent ㅇ (있어 = isseo), and ㄹ before a vowel is r.
fn romanize_korean(s: &str) -> String {
    const INITIAL: [&str; 19] = ["g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h"];
    const VOWEL: [&str; 21] = [
        "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i",
    ];
    // Final consonant at the end of a syllable / when it moves onto a following vowel.
    const FINAL: [&str; 28] = ["", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "p", "l", "l", "p", "l", "m", "p", "p", "t", "t", "ng", "t", "t", "k", "t", "p", "t"];
    const LINKED: [&str; 28] = [
        "", "g", "kk", "ks", "n", "nj", "n", "d", "r", "lg", "lm", "lb", "ls", "lt", "lp", "r", "m", "b", "ps", "s", "ss", "ng", "j", "ch", "k", "t", "p", "",
    ];
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut carried = false; // this syllable's silent ㅇ already got the previous final
    for (i, &c) in chars.iter().enumerate() {
        if !is_hangul(c) {
            out.push(c);
            carried = false;
            continue;
        }
        let code = c as u32 - 0xac00;
        let (l, v, t) = ((code / 588) as usize, ((code % 588) / 28) as usize, (code % 28) as usize);
        if !(carried && l == 11) {
            out.push_str(INITIAL[l]);
        }
        out.push_str(VOWEL[v]);
        let next_silent = chars.get(i + 1).filter(|n| is_hangul(**n)).is_some_and(|n| (*n as u32 - 0xac00) / 588 == 11);
        carried = next_silent && t != 0;
        out.push_str(if carried { LINKED[t] } else { FINAL[t] });
    }
    out
}

fn romanize_chinese(s: &str) -> String {
    use pinyin::ToPinyin;
    let mut out = String::new();
    for (c, p) in s.chars().zip(s.to_pinyin()) {
        match p {
            Some(p) => {
                if !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push_str(p.with_tone());
            }
            None => out.push(c),
        }
    }
    out
}

fn romanize_cyrillic(s: &str) -> String {
    s.chars()
        .map(|c| {
            let lower = c.to_lowercase().next().unwrap_or(c);
            let r = match lower {
                'а' => "a", 'б' => "b", 'в' => "v", 'г' => "g", 'д' => "d", 'е' => "e", 'ё' => "yo", 'ж' => "zh",
                'з' => "z", 'и' => "i", 'й' => "y", 'к' => "k", 'л' => "l", 'м' => "m", 'н' => "n", 'о' => "o",
                'п' => "p", 'р' => "r", 'с' => "s", 'т' => "t", 'у' => "u", 'ф' => "f", 'х' => "kh", 'ц' => "ts",
                'ч' => "ch", 'ш' => "sh", 'щ' => "shch", 'ъ' | 'ь' => "", 'ы' => "y", 'э' => "e", 'ю' => "yu", 'я' => "ya",
                'і' => "i", 'ї' => "yi", 'є' => "ye", 'ґ' => "g",
                _ => return c.to_string(),
            };
            if c != lower {
                let mut cs = r.chars();
                cs.next().map(|f| f.to_uppercase().chain(cs).collect()).unwrap_or_default()
            } else {
                r.to_string()
            }
        })
        .collect()
}

fn seek_to(state: &Shared, secs: f64) {
    let st = state.borrow();
    if let (Some(p), Some(id)) = (st.player.clone(), st.track_id.clone()) {
        glib::spawn_future_local(async move {
            let _ = p.set_position(&id, (secs * 1e6) as i64).await;
        });
    }
}

fn mmss(secs: i64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn minimal_flag() -> PathBuf {
    glib::user_config_dir().join("nusa/minimal")
}

fn no_mascot_flag() -> PathBuf {
    glib::user_config_dir().join("nusa/no-mascot")
}

fn autostart_file() -> PathBuf {
    glib::user_config_dir().join("autostart/nusa-linux.desktop")
}

enum TrayMsg {
    Minimal(bool),
    Mascot(bool),
    Quit,
}

struct Tray {
    minimal: bool,
    mascot: bool,
    tx: async_channel::Sender<TrayMsg>,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "nusa-linux".into()
    }
    fn title(&self) -> String {
        "Nusa".into()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Nusa".into(), ..Default::default() }
    }
    // A white notch-shaped pill, drawn here so the icon doesn't depend on the icon theme.
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let n = 32;
        let mut data = Vec::with_capacity(n * n * 4);
        for y in 0..n {
            for x in 0..n {
                // Distance to the segment (10,16)-(22,16); pill radius 7, 1px antialias.
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let d = ((fx - fx.clamp(10.0, 22.0)).powi(2) + (fy - 16.0).powi(2)).sqrt();
                let a = ((7.5 - d).clamp(0.0, 1.0) * 255.0) as u8;
                data.extend_from_slice(&[a, 255, 255, 255]); // ARGB
            }
        }
        vec![ksni::Icon { width: n as i32, height: n as i32, data }]
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, StandardItem};
        vec![
            CheckmarkItem {
                label: "Start with login".into(),
                checked: autostart_file().exists(),
                activate: Box::new(|_| {
                    let f = autostart_file();
                    let _ = if f.exists() {
                        std::fs::remove_file(&f)
                    } else {
                        let exe = std::env::current_exe().unwrap_or_default();
                        std::fs::create_dir_all(f.parent().unwrap()).and_then(|_| {
                            std::fs::write(&f, format!("[Desktop Entry]\nType=Application\nName=Nusa\nExec={}\n", exe.display()))
                        })
                    };
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Minimal notch".into(),
                checked: self.minimal,
                activate: Box::new(|t: &mut Self| {
                    t.minimal = !t.minimal;
                    let _ = t.tx.send_blocking(TrayMsg::Minimal(t.minimal));
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Mascot".into(),
                checked: self.mascot,
                activate: Box::new(|t: &mut Self| {
                    t.mascot = !t.mascot;
                    let _ = t.tx.send_blocking(TrayMsg::Mascot(t.mascot));
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Exit".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send_blocking(TrayMsg::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn spawn_tray(app: &gtk::Application, ui: &Ui, state: &Shared) {
    use ksni::blocking::TrayMethods;
    let (tx, rx) = async_channel::unbounded();
    let handle = match (Tray { minimal: state.borrow().minimal, mascot: !state.borrow().mascot_off, tx }).spawn() {
        Ok(h) => h,
        Err(e) => return eprintln!("tray: {e}"),
    };
    let (app, ui, state) = (app.clone(), ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        let _handle = handle; // tray lives as long as this loop
        while let Ok(msg) = rx.recv().await {
            match msg {
                TrayMsg::Minimal(on) => {
                    let f = minimal_flag();
                    let _ = if on {
                        std::fs::create_dir_all(f.parent().unwrap()).and_then(|_| std::fs::write(&f, ""))
                    } else {
                        std::fs::remove_file(&f)
                    };
                    state.borrow_mut().minimal = on;
                    refresh_compact(&ui, &state.borrow());
                }
                TrayMsg::Mascot(on) => {
                    let f = no_mascot_flag();
                    let _ = if on {
                        std::fs::remove_file(&f)
                    } else {
                        std::fs::create_dir_all(f.parent().unwrap()).and_then(|_| std::fs::write(&f, ""))
                    };
                    state.borrow_mut().mascot_off = !on;
                    if !on {
                        set_mood(&ui, &state, Mood::None);
                    }
                }
                TrayMsg::Quit => app.quit(),
            }
        }
    });
}

/// Cover image bytes from an MPRIS artUrl (file:// from local players, https:// from browsers).
fn load_art(url: &str) -> Option<Vec<u8>> {
    if url.starts_with("http") {
        let mut resp = ureq::get(url).call().ok()?;
        resp.body_mut().with_config().limit(10 << 20).read_to_vec().ok()
    } else {
        std::fs::read(gio::File::for_uri(url).path()?).ok()
    }
}

fn fetch_lyrics(title: &str, artist: &str, duration_s: i64) -> Vec<(i64, String)> {
    let mut req = ureq::get("https://lrclib.net/api/get")
        .header("User-Agent", "nusa-linux (personal use)")
        .query("track_name", title)
        .query("artist_name", artist);
    if duration_s > 0 {
        req = req.query("duration", duration_s.to_string());
    }
    let Ok(mut resp) = req.call() else { return vec![] };
    let Ok(body) = resp.body_mut().read_to_string() else { return vec![] };
    let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    json["syncedLyrics"].as_str().map(parse_lrc).unwrap_or_default()
}

/// "[mm:ss.xx] text" lines into (ms, text), sorted.
fn parse_lrc(lrc: &str) -> Vec<(i64, String)> {
    let mut out: Vec<_> = lrc
        .lines()
        .filter_map(|l| {
            let (stamp, text) = l.strip_prefix('[')?.split_once(']')?;
            let (m, s) = stamp.split_once(':')?;
            let ms = m.parse::<i64>().ok()? * 60_000 + (s.parse::<f64>().ok()? * 1000.0) as i64;
            Some((ms, text.trim().to_string()))
        })
        .collect();
    out.sort_by_key(|(ms, _)| *ms);
    out
}

fn current_index(lyrics: &[(i64, String)], ms: i64) -> Option<usize> {
    lyrics.partition_point(|(t, _)| *t <= ms).checked_sub(1)
}

fn current_line(lyrics: &[(i64, String)], ms: i64) -> Option<String> {
    Some(lyrics.get(current_index(lyrics, ms)?)?.1.clone())
}

#[test]
fn lrc() {
    let l = parse_lrc("[00:12.50] second\n[ar:Someone]\n[00:01.00] first\n[01:00.00]");
    assert_eq!(l, vec![(1000, "first".into()), (12500, "second".into()), (60000, "".into())]);
    assert_eq!(current_line(&l, 500), None);
    assert_eq!(current_line(&l, 12500).as_deref(), Some("second"));
    assert_eq!(current_line(&l, 30000).as_deref(), Some("second"));
    assert_eq!(current_index(&l, 60000), Some(2));
}

#[test]
fn romanization() {
    assert_eq!(romanize_korean("사랑해"), "saranghae");
    assert_eq!(romanize_korean("있어"), "isseo");
    assert_eq!(romanize_korean("우리 사랑하지 말아요"), "uri saranghaji marayo");
    assert_eq!(romanize_korean("감사합니다"), "gamsahapnida");
    assert_eq!(romanize_chinese("我爱你"), "wǒ ài nǐ");
    assert_eq!(romanize_cyrillic("Привет, мир"), "Privet, mir");
    let song = [(0, "君の名は".to_string()), (1, "名前".to_string())];
    assert_eq!(romanize_song(&song), vec![None, None]); // Japanese: kanji not read as Mandarin
    assert_eq!(romanize_song(&[(0, "hello".to_string())]), vec![None]);
}

#[test]
fn accent_is_dark_and_keeps_hue() {
    let red = [200u8, 20, 20].repeat(4);
    let (r, g, b) = accent_color(&red, 3, 6, 2);
    assert!(r == 110 && g < 20 && b < 20, "{r} {g} {b}");
}

#[test]
fn tanggal_indonesia() {
    let d = glib::DateTime::from_local(2026, 10, 26, 9, 0, 0.0).unwrap();
    assert_eq!(tanggal(&d), "Senin, 26 Oktober 2026");
    let d = glib::DateTime::from_local(2026, 2, 1, 9, 0, 0.0).unwrap();
    assert_eq!(tanggal(&d), "Minggu, 1 Februari 2026");
}
