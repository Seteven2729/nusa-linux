use futures_util::StreamExt;
use gtk::{gio, glib, prelude::*};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};
use std::path::PathBuf;
use zbus::{proxy, Connection, MatchRule, MessageStream, ObjectPath, OwnedObjectPath, OwnedVariant};

mod capture;
mod config;
mod dbus;
mod style;
mod media;
mod lyrics;
mod system;
mod mascot;
mod tray;

use {capture::*, config::*, dbus::*, lyrics::*, mascot::*, media::*, style::*, system::*, tray::*};

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
    cam: gtk::Label,
    share: gtk::Label,
    stats: gtk::Label,
    clips: gtk::Box,
    clip_btn: gtk::ToggleButton,
    accent: gtk::CssProvider,
    wall: gtk::CssProvider,
    dl_bar: gtk::ProgressBar,
    /// Open view has two pages: "home" (clock, stats, clipboard) and "media" (player).
    pages: gtk::Stack,
    page_prev: gtk::Button,
    page_next: gtk::Button,
    clock: gtk::Label,
    date: gtk::Label,
    no_media: gtk::Label,
    head: gtk::Box,
    dl_active: gtk::Box,
    dl_recent: gtk::Box,
    pager: gtk::Box,
    top_row: gtk::Revealer,
    /// Holds the notch's min-width while opening/closing; empty means the stylesheet default.
    width_css: gtk::CssProvider,
    /// Hover wants the notch open (true even during the widen-first phase).
    want_open: Rc<std::cell::Cell<bool>>,
    /// Bumped on every open/close so a stale delayed step can tell it was superseded.
    anim: Rc<std::cell::Cell<u32>>,
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
    /// Apps holding a camera device open directly.
    cam_apps: Vec<String>,
    /// A camera is streaming through PipeWire.
    cam_pw: bool,
    /// The screen is being shared (a non-camera PipeWire video source exists).
    sharing: bool,
    /// Download being shown: (app name, progress 0..1).
    download: Option<(String, f64)>,
    /// Every download in progress, by app, for the downloads page.
    downloads: Vec<(String, f64)>,
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
    // Green while a camera is on, red while the screen is shared, as on phones.
    let cam = gtk::Label::builder().label("●").css_classes(["cam"]).visible(false).build();
    let share = gtk::Label::builder().label("●").css_classes(["share"]).visible(false).build();
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    top.append(&share);
    top.append(&cam);
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
    // and so the home page doesn't grow wide.
    let clip_btn = gtk::ToggleButton::builder()
        .icon_name("edit-paste-symbolic")
        .tooltip_text("Clipboard")
        .css_classes(["panel-toggle"])
        .valign(gtk::Align::Center)
        .visible(false)
        .build();

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&prev);
    buttons.append(&play);
    buttons.append(&next);
    // Media page: the player, or a short note when nothing is playing.
    let no_media = gtk::Label::builder().label("Tidak ada media yang diputar").css_classes(["no-media"]).build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.append(&head);
    body.append(&seek_row);
    body.append(&buttons);
    body.append(&no_media);

    // Home page: big clock, date, system stats, and the clipboard button.
    let clock = gtk::Label::builder().css_classes(["title", "clock"]).build();
    let date = gtk::Label::builder().css_classes(["artist"]).build();
    let stats = gtk::Label::builder().css_classes(["stats"]).build();
    let clock_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    clock_box.set_hexpand(true);
    clock_box.append(&clock);
    clock_box.append(&date);
    let home_head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    home_head.append(&clock_box);
    home_head.append(&clip_btn);
    let home_body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    home_body.append(&home_head);
    home_body.append(&stats);
    let clips = gtk::Box::new(gtk::Orientation::Vertical, 4);
    clips.set_width_request(260);
    clips.set_valign(gtk::Align::Start);
    clips.set_visible(false);
    // Full lyrics sit to the right of the player when the lyrics button is on.
    let lyrics_list = gtk::ListBox::new();
    lyrics_list.set_selection_mode(gtk::SelectionMode::None);
    let lyrics_scroll = gtk::ScrolledWindow::builder()
        .child(&lyrics_list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        // min-content-width is ignored without a horizontal scrollbar, so size it directly.
        .width_request(260)
        .height_request(150)
        .visible(false)
        .build();
    // Downloads page: everything in progress, then the newest files in ~/Downloads.
    let section = |text: &str| gtk::Label::builder().label(text).css_classes(["section"]).xalign(0.0).build();
    let dl_active = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let dl_recent = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let open_folder = gtk::Button::builder().label("📂  Buka folder Download").css_classes(["clip"]).build();
    open_folder.connect_clicked(|_| {
        let _ = gio::AppInfo::launch_default_for_uri(&gio::File::for_path(downloads_dir()).uri(), None::<&gio::AppLaunchContext>);
    });
    let downloads_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    downloads_page.set_width_request(300);
    downloads_page.append(&section("Sedang diunduh"));
    downloads_page.append(&dl_active);
    downloads_page.append(&section("Terbaru di Downloads"));
    downloads_page.append(&dl_recent);
    downloads_page.append(&open_folder);
    let media_page = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    media_page.append(&body);
    media_page.append(&lyrics_scroll);
    let home_page = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    home_page.append(&home_body);
    home_page.append(&clips);
    // Pages slide sideways, and the notch eases to the new page's size instead of jumping.
    let pages = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::SlideLeftRight)
        .transition_duration(220)
        .interpolate_size(true)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .hexpand(true)
        .build();
    pages.add_named(&home_page, Some("home"));
    pages.add_named(&media_page, Some("media"));
    if cfg().downloads {
        pages.add_named(&downloads_page, Some("downloads"));
    }
    let arrow = |icon: &str| {
        gtk::Button::builder().icon_name(icon).css_classes(["page-arrow"]).valign(gtk::Align::Center).build()
    };
    let (page_prev, page_next) = (arrow("go-previous-symbolic"), arrow("go-next-symbolic"));
    let pager = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    pager.set_margin_top(10);
    pager.append(&page_prev);
    pager.append(&pages);
    pager.append(&page_next);
    let expanded = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(200)
        .child(&pager)
        .build();

    let notch = gtk::Box::new(gtk::Orientation::Vertical, 2);
    notch.add_css_class("notch");
    // The top row collapses with its own slide, so while the open content slides in the
    // notch's height never dips (hiding the row instantly flashed a 2px-tall notch).
    let top_row = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(200)
        .reveal_child(true)
        .child(&top)
        .build();
    notch.append(&top_row);
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
        cam,
        share,
        stats,
        clips,
        clip_btn,
        accent: gtk::CssProvider::new(),
        wall: gtk::CssProvider::new(),
        dl_bar,
        pages,
        page_prev,
        page_next,
        clock,
        date,
        no_media,
        head,
        dl_active,
        dl_recent,
        pager,
        top_row,
        width_css: gtk::CssProvider::new(),
        want_open: Default::default(),
        anim: Default::default(),
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
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &ui.width_css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 2,
    );
    // A collapsed GtkRevealer still reserves its child's width, which kept the closed notch
    // as wide as the open one. Hide the content entirely while closed.
    ui.pager.set_visible(false);
    // Closing, second step: once the content has slid up, hide it and let the notch narrow back.
    let u = ui.clone();
    ui.expanded.connect_child_revealed_notify(move |r| {
        if !r.is_child_revealed() && !u.want_open.get() {
            u.pager.set_visible(false);
            u.width_css.load_from_string("");
        }
    });
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
    if cfg().camera {
        spawn_capture_watch(&ui, &state);
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

    // Read ~/Downloads only when the downloads page comes into view, not on every tick.
    let (u, s) = (ui.clone(), state.clone());
    ui.pages.connect_visible_child_name_notify(move |p| {
        if p.visible_child_name().as_deref() == Some("downloads") {
            fill_downloads_page(&u, &s.borrow());
        }
    });
    // Arrows step through the pages in order.
    for (btn, step) in [(&ui.page_prev, -1i32), (&ui.page_next, 1)] {
        let (u, s) = (ui.clone(), state.clone());
        btn.connect_clicked(move |_| {
            let order = page_order();
            let i = order.iter().position(|p| Some(*p) == u.pages.visible_child_name().as_deref()).unwrap_or(0) as i32;
            if let Some(page) = order.get((i + step).max(0) as usize) {
                u.pages.set_visible_child_name(page);
            }
            refresh_compact(&u, &s.borrow());
        });
    }

    // Hover opens at once; leaving closes only after a short grace period. While the notch
    // grows, the Wayland surface is resized under a still pointer and GTK reports a leave and
    // a new enter. Closing on that leave made the notch flicker open/shut near its edges.
    let hover = gtk::EventControllerMotion::new();
    let hover_gen = Rc::new(std::cell::Cell::new(0u32));
    let (u1, s1, g1) = (ui.clone(), state.clone(), hover_gen.clone());
    hover.connect_enter(move |_, _, _| {
        g1.set(g1.get() + 1);
        if !u1.want_open.get() {
            // Open on the page that matters right now, without a sideways slide.
            let page = {
                let st = s1.borrow();
                if st.player.is_some() && !st.track.is_empty() {
                    "media"
                } else if !st.downloads.is_empty() && cfg().downloads {
                    "downloads"
                } else {
                    "home"
                }
            };
            u1.pages.set_visible_child_full(page, gtk::StackTransitionType::None);
            set_open(&u1, &s1, true);
        }
    });
    let (u2, s2) = (ui.clone(), state.clone());
    hover.connect_leave(move |_| {
        let seq = hover_gen.get() + 1;
        hover_gen.set(seq);
        let (u, s, g) = (u2.clone(), s2.clone(), hover_gen.clone());
        glib::timeout_add_local_once(Duration::from_millis(250), move || {
            if g.get() == seq {
                set_open(&u, &s, false);
            }
        });
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
        if u.stats.is_visible() && u.expanded.reveals_child() && u.pages.visible_child_name().as_deref() == Some("home") {
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
    // Fade new text in, except the clock ticking over to the next minute.
    let old = ui.compact.label();
    let is_clock = |t: &str| t.len() == 5 && t.as_bytes()[2] == b':';
    if old != text && !(is_clock(&old) && is_clock(&text)) {
        retrigger(&ui.compact, "fade", 250);
    }
    ui.compact.set_label(&text);
    // Minimal mode: closed and quiet, the notch is just a thin black line. Lyrics and alerts still show.
    let open = ui.expanded.reveals_child();
    // A player that is open but stopped (no title) counts as idle, same as no player at all.
    let has_track = st.player.is_some() && !st.track.is_empty();
    let has_lyrics = has_track && !st.lyrics.is_empty();
    let page = ui.pages.visible_child_name();
    let on_media = page.as_deref() == Some("media");
    let full_lyrics = open && on_media && has_lyrics && ui.lyrics_btn.is_active();
    ui.lyrics_btn.set_visible(has_lyrics);
    ui.lyrics_scroll.set_visible(full_lyrics);
    ui.lyric.set_visible(st.playing && !st.lyrics.is_empty() && st.alert.is_none() && !full_lyrics);
    ui.lyric_rom.set_visible(ui.lyric.is_visible() && st.lyrics_rom.iter().any(Option::is_some));
    ui.stats.set_visible(cfg().stats);
    let has_clips = ui.clips.first_child().is_some();
    ui.clip_btn.set_visible(has_clips);
    ui.clips.set_visible(open && !on_media && has_clips && ui.clip_btn.is_active());
    let order = page_order();
    let i = order.iter().position(|p| Some(*p) == page.as_deref()).unwrap_or(0);
    ui.page_prev.set_sensitive(i > 0);
    ui.page_next.set_sensitive(i + 1 < order.len());
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
    ui.top_row.set_reveal_child(show_compact);
    // Items inside the row keep their visibility while it slides away, so the slide has
    // something to animate; the row's revealer alone decides whether it shows.
    ui.mic.set_visible(!st.mic_apps.is_empty());
    ui.cam.set_visible(!st.cam_apps.is_empty() || st.cam_pw);
    ui.share.set_visible(st.sharing);
    // The mascot stands in for the clock (or the song title) once you have been away a while.
    let mascot = st.mood != Mood::None && st.alert.is_none() && st.download.is_none();
    ui.dl_bar.set_visible(!open && st.download.is_some());
    if let Some((_, p)) = &st.download {
        ui.dl_bar.set_fraction(*p);
    }
    ui.mascot.set_visible(mascot);
    ui.compact.set_visible(!mascot);
    let media = st.playing && st.alert.is_none();
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
    // Media page: the player, or the "nothing playing" note.
    ui.head.set_visible(has_track);
    ui.buttons.set_visible(has_track);
    ui.no_media.set_visible(!has_track);
    if !has_track {
        ui.seek.parent().unwrap().set_visible(false);
    }
    // Home page clock.
    let now = glib::DateTime::now_local().ok();
    ui.clock.set_label(&now.as_ref().and_then(|d| d.format("%H:%M").ok()).unwrap_or_default());
    ui.date.set_label(&now.as_ref().map(tanggal).unwrap_or_default());
}

/// Open or close the notch without the width ever jumping.
/// Opening widens the closed notch to the open content's width first (a CSS min-width
/// transition), then slides the content down. Closing slides it up first; the
/// child-revealed handler then lets the notch narrow back.
fn set_open(ui: &Ui, state: &Shared, open: bool) {
    if ui.want_open.get() == open {
        return;
    }
    ui.want_open.set(open);
    let seq = ui.anim.get() + 1;
    ui.anim.set(seq);
    if open {
        refresh_compact(ui, &state.borrow()); // so the measurement sees the right page contents
        // Measure the open content without showing it (visible on and off within this frame).
        ui.pager.set_visible(true);
        let (_, natural, _, _) = ui.pager.measure(gtk::Orientation::Horizontal, -1);
        ui.pager.set_visible(false);
        ui.width_css.load_from_string(&format!(".notch {{ min-width: {natural}px; }}"));
        let (u, s) = (ui.clone(), state.clone());
        glib::timeout_add_local_once(Duration::from_millis(170), move || {
            if u.anim.get() == seq {
                u.pager.set_visible(true);
                u.expanded.set_reveal_child(true);
                refresh_compact(&u, &s.borrow());
            }
        });
    } else {
        ui.expanded.set_reveal_child(false);
        ui.clip_btn.set_active(false);
        if !ui.expanded.is_child_revealed() {
            // Closed during the widen phase: nothing to slide up, narrow right away.
            ui.pager.set_visible(false);
            ui.width_css.load_from_string("");
        }
        refresh_compact(ui, &state.borrow());
    }
}

/// Pages left to right, as the arrows walk them.
fn page_order() -> Vec<&'static str> {
    let mut order = vec!["home", "media"];
    if cfg().downloads {
        order.push("downloads");
    }
    order
}

/// Restart a one-shot CSS animation by toggling its class.
fn retrigger(widget: &impl IsA<gtk::Widget>, class: &'static str, ms: u64) {
    let w = widget.as_ref().clone();
    w.remove_css_class(class);
    w.add_css_class(class);
    glib::timeout_add_local_once(Duration::from_millis(ms), move || w.remove_css_class(class));
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
    // A small bounce draws the eye to a new alert.
    retrigger(&ui.notch, "pop", 350);
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
