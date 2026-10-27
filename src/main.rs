use futures_util::StreamExt;
use gtk::{gio, glib, prelude::*};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};
use std::path::PathBuf;
use zbus::{proxy, Connection, MatchRule, MessageStream, ObjectPath, OwnedObjectPath, OwnedVariant};

mod config;
mod dbus;
mod style;
mod media;
mod lyrics;
mod system;
mod mascot;
mod tray;

use {config::*, dbus::*, lyrics::*, mascot::*, media::*, style::*, system::*, tray::*};

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
    clip_btn: gtk::ToggleButton,
    accent: gtk::CssProvider,
    wall: gtk::CssProvider,
    dl_bar: gtk::ProgressBar,
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
    /// Mascot head bob while music plays: flips between up and down on the beat timer.
    bob: bool,
    mascot_off: bool,
    /// Romanized lyric lines, same order as `lyrics`; None where a line needs none.
    lyrics_rom: Vec<Option<String>>,
    /// Previous (busy, total) jiffies from /proc/stat, for the CPU percentage.
    cpu_prev: Option<(u64, u64)>,
    /// Apps recording from a microphone right now.
    mic_apps: Vec<String>,
    /// Download being shown: (app name, progress 0..1).
    download: Option<(String, f64)>,
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
    let [c1, c2, c3] = &cfg().gradient;
    css.load_from_string(&CSS.replace("#020510 0%, #061338 65%, #0c225c 100%", &format!("{c1} 0%, {c2} 65%, {c3} 100%")));
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
        .css_classes(["panel-toggle"])
        .valign(gtk::Align::Center)
        .build();
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    head.append(&art);
    head.append(&text);
    head.append(&lyrics_btn);
    // Clipboard history stays behind a button so it never shows up unasked (screen sharing),
    // and so the idle notch doesn't grow wide.
    let clip_btn = gtk::ToggleButton::builder()
        .icon_name("edit-paste-symbolic")
        .tooltip_text("Clipboard")
        .css_classes(["panel-toggle"])
        .valign(gtk::Align::Center)
        .visible(false)
        .build();
    head.append(&clip_btn);

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
    clips.set_width_request(260);
    clips.set_margin_top(10);
    clips.set_valign(gtk::Align::Start);
    clips.set_visible(false);
    body.append(&head);
    body.append(&stats);
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
    split.append(&clips);
    let expanded = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(200)
        .child(&split)
        .build();

    let notch = gtk::Box::new(gtk::Orientation::Vertical, 2);
    notch.add_css_class("notch");
    notch.append(&top);
    // Thin download bar under the top row; stays even with music or in minimal mode.
    let dl_bar = gtk::ProgressBar::builder().css_classes(["dl"]).visible(false).build();
    notch.append(&dl_bar);
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
        clip_btn,
        accent: gtk::CssProvider::new(),
        wall: gtk::CssProvider::new(),
        dl_bar,
    };
    // Cover-tinted gradient lives in its own provider so it can be swapped per track.
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &ui.accent,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &ui.wall,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    let state: Shared = Rc::new(RefCell::new(State {
        minimal: cfg().minimal,
        mascot_off: !cfg().mascot,
        ..Default::default()
    }));

    let s = state.clone();
    ui.mascot.set_draw_func(move |_, cr, w, h| {
        let st = s.borrow();
        let music = if st.playing {
            Music::Playing(st.bob)
        } else if st.player.is_some() && !st.track.is_empty() {
            Music::Paused
        } else {
            Music::None
        };
        draw_eyes(cr, w as f64, h as f64, st.mood, st.blink, music);
    });
    let (u, s) = (ui.clone(), state.clone());
    ui.lyrics_btn.connect_toggled(move |_| refresh_compact(&u, &s.borrow()));
    let (u, s) = (ui.clone(), state.clone());
    ui.clip_btn.connect_toggled(move |_| refresh_compact(&u, &s.borrow()));
    let s = state.clone();
    ui.lyrics_list.connect_row_activated(move |_, row| {
        let ms = s.borrow().lyrics.get(row.index() as usize).map(|l| l.0);
        if let Some(ms) = ms {
            seek_to(&s, ms as f64 / 1000.0);
        }
    });
    spawn_idle_watch(&ui, &state);
    // While the mascot is out and music plays, bob its head at the same 4 Hz as the equalizer.
    let (u, s) = (ui.clone(), state.clone());
    glib::timeout_add_local(Duration::from_millis(250), move || {
        if u.mascot.is_visible() && s.borrow().playing {
            let bob = !s.borrow().bob;
            s.borrow_mut().bob = bob;
            u.mascot.queue_draw();
        }
        glib::ControlFlow::Continue
    });
    if cfg().follow_wallpaper {
        let u = ui.clone();
        glib::spawn_future_local(async move {
            if let Err(e) = wallpaper_loop(u).await {
                eprintln!("wallpaper: {e}");
            }
        });
    }
    if cfg().mic {
        spawn_mic_watch(&ui, &state);
    }

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
    if cfg().gestures {
        notch.add_controller(scroll);
    }
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
    if cfg().gestures {
        notch.add_controller(middle);
    }

    // Hover opens, leaving closes.
    let hover = gtk::EventControllerMotion::new();
    let (u1, s1, u2, s2) = (ui.clone(), state.clone(), ui.clone(), state.clone());
    hover.connect_enter(move |_, _, _| {
        u1.expanded.set_reveal_child(true);
        refresh_compact(&u1, &s1.borrow());
    });
    hover.connect_leave(move |_| {
        u2.expanded.set_reveal_child(false);
        u2.clip_btn.set_active(false);
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
    migrate_autostart();

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
    if cfg().battery {
        let (u, s) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            if let Err(e) = battery_loop(u, s).await {
                eprintln!("battery: {e}");
            }
        });
    }
    if cfg().network {
        let (u, s) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            if let Err(e) = network_loop(u, s).await {
                eprintln!("network: {e}");
            }
        });
    }
    if cfg().bluetooth {
        let (u, s) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            if let Err(e) = bluetooth_loop(u, s).await {
                eprintln!("bluetooth: {e}");
            }
        });
    }
    if cfg().clipboard {
        let (u, s) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            if let Err(e) = clipboard_loop(u, s).await {
                eprintln!("clipboard: {e}");
            }
        });
    }
    if cfg().phone {
        let (u, s) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            if let Err(e) = phone_loop(u, s).await {
                eprintln!("kdeconnect: {e}");
            }
        });
    }
    if cfg().downloads {
        let (u, s) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            if let Err(e) = download_loop(u, s).await {
                eprintln!("downloads: {e}");
            }
        });
    }

    window.present();
}

fn refresh_compact(ui: &Ui, st: &State) {
    let text = if let Some(a) = &st.alert {
        a.clone()
    } else if st.playing {
        format!("♪  {}", st.track)
    } else if let Some((_, p)) = &st.download {
        format!("⬇  {}%", (p * 100.0).round())
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
    ui.stats.set_visible(!has_track && cfg().stats);
    let has_clips = ui.clips.first_child().is_some();
    ui.clip_btn.set_visible(!has_track && has_clips);
    ui.clips.set_visible(open && !has_track && has_clips && ui.clip_btn.is_active());
    // Gradient source: album cover while a track is on, wallpaper when idle, else the config colors.
    let tinted = has_track && cfg().follow_cover && ui.art.paintable().is_some();
    let wall = !has_track && cfg().follow_wallpaper;
    for (class, on) in [("tinted", tinted), ("wall", wall)] {
        if on {
            ui.notch.add_css_class(class);
        } else {
            ui.notch.remove_css_class(class);
        }
    }
    // When open, the expanded view shows either the track or a big clock, so the top row would only repeat it.
    let show_compact = st.alert.is_some() || (!open && !st.minimal);
    ui.mic.set_visible(!st.mic_apps.is_empty() && show_compact);
    // The mascot stands in for the clock (or the song title) once you have been away a while.
    let mascot = st.mood != Mood::None && show_compact && st.alert.is_none() && st.download.is_none();
    ui.dl_bar.set_visible(!open && st.download.is_some());
    if let Some((_, p)) = &st.download {
        ui.dl_bar.set_fraction(*p);
    }
    ui.mascot.set_visible(mascot);
    ui.compact.set_visible(show_compact && !mascot);
    let media = show_compact && st.playing && st.alert.is_none();
    ui.eq.set_visible(media);
    ui.mini_art.set_visible(media && !mascot && ui.mini_art.paintable().is_some());
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

const HARI: [&str; 7] = ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"];
const BULAN: [&str; 12] =
    ["Januari", "Februari", "Maret", "April", "Mei", "Juni", "Juli", "Agustus", "September", "Oktober", "November", "Desember"];

/// "Senin, 26 Oktober 2026", independent of the system locale.
fn tanggal(d: &glib::DateTime) -> String {
    let (day, month) = (HARI[d.day_of_week() as usize - 1], BULAN[d.month() as usize - 1]);
    format!("{day}, {} {month} {}", d.day_of_month(), d.year())
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

#[test]
fn tanggal_indonesia() {
    let d = glib::DateTime::from_local(2026, 10, 26, 9, 0, 0.0).unwrap();
    assert_eq!(tanggal(&d), "Senin, 26 Oktober 2026");
    let d = glib::DateTime::from_local(2026, 2, 1, 9, 0, 0.0).unwrap();
    assert_eq!(tanggal(&d), "Minggu, 1 Februari 2026");
}
