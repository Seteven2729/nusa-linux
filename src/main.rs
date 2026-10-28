use futures_util::StreamExt;
use gtk::{gio, glib, prelude::*};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::path::PathBuf;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};
use zbus::{
    Connection, MatchRule, MessageStream, ObjectPath, OwnedObjectPath, OwnedVariant, proxy,
};

mod capture;
mod config;
mod dbus;
mod lyrics;
mod mascot;
mod media;
mod mixer;
mod screenshots;
mod style;
mod system;
mod tray;

use {
    capture::*, config::*, dbus::*, lyrics::*, mascot::*, media::*, mixer::*, screenshots::*,
    style::*, system::*, tray::*,
};

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
    lyric_rev: gtk::Revealer,
    expanded: gtk::Revealer,
    title: gtk::Label,
    artist: gtk::Label,
    play: gtk::Button,
    buttons: gtk::Box,
    mascot: gtk::DrawingArea,
    lyrics_btn: gtk::ToggleButton,
    lyrics_list: gtk::ListBox,
    lyrics_scroll: gtk::ScrolledWindow,
    lyrics_rev: gtk::Revealer,
    lyric_rom: gtk::Label,
    mic: gtk::Label,
    cam: gtk::Label,
    share: gtk::Label,
    stats_box: gtk::Grid,
    stat_bars: [gtk::ProgressBar; 3],
    stat_vals: [gtk::Label; 3],
    timer_entry: gtk::Entry,
    timer_btn: gtk::Button,
    timer_badge: gtk::Label,
    song_bar: gtk::ProgressBar,
    clips: gtk::Box,
    clip_stack: gtk::Stack,
    clips_rev: gtk::Revealer,
    clip_btn: gtk::ToggleButton,
    accent: gtk::CssProvider,
    wall: gtk::CssProvider,
    dl_bar: gtk::ProgressBar,
    dl_row: gtk::Box,
    /// Open view pages: "home", "media", "mixer", "downloads", "screenshots".
    pages: gtk::Stack,
    page_pills: Vec<(&'static str, gtk::Button)>,
    clock: gtk::Label,
    date: gtk::Label,
    no_media: gtk::Label,
    head: gtk::Box,
    dl_active: gtk::Box,
    dl_recent: gtk::Box,
    player_btn: gtk::Button,
    mixer_box: gtk::Box,
    shots_row: gtk::Box,
    /// Screenshot thumbnail shown next to the "saved" alert.
    shot_thumb: gtk::Image,
    qr_pic: gtk::Picture,
    pager: gtk::Box,
    top_row: gtk::Revealer,
    /// The themed stylesheet; reloaded when a theme is picked from the tray.
    css: gtk::CssProvider,
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
    /// Download being shown: (name, status label, progress 0..1).
    download: Option<(String, String, f64)>,
    /// Every open MPRIS player as (bus owner, display name).
    players: Vec<(String, String)>,
    /// Player chosen with the switch button; kept until it closes.
    pinned: Option<String>,
    /// Newest screenshots, newest first (paths in ~/Pictures/Screenshots).
    shots: Vec<PathBuf>,
    /// Picture shown with the current alert (a new screenshot's thumbnail).
    alert_thumb: Option<gtk::gdk::Texture>,
    /// Asks the MPRIS loop to pick again (after the switch button changes `pinned`).
    repick: Option<async_channel::Sender<()>>,
    /// Every download in progress as (name, status label, progress 0..1), for the downloads page.
    downloads: Vec<(String, String, f64)>,
    /// Remaining countdown seconds for the quick timer (0 when stopped).
    timer_rem: u32,
    /// Preset minutes for the quick timer (1..=180).
    timer_mins: u32,
    /// True while the user is actively typing into `timer_entry`.
    editing_timer: bool,
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
    css.load_from_string(&themed_css(cfg()));
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
        let p = gtk::Image::builder()
            .pixel_size(size)
            .css_classes([class])
            .build();
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
    let mascot = gtk::DrawingArea::builder()
        .content_width(44)
        .content_height(16)
        .visible(false)
        .build();
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
    let mic = gtk::Label::builder()
        .label("●")
        .css_classes(["mic"])
        .visible(false)
        .build();
    // Green while a camera is on, red while the screen is shared, as on phones.
    let cam = gtk::Label::builder()
        .label("●")
        .css_classes(["cam"])
        .visible(false)
        .build();
    let share = gtk::Label::builder()
        .label("●")
        .css_classes(["share"])
        .visible(false)
        .build();
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    top.append(&share);
    top.append(&cam);
    top.append(&mic);
    let shot_thumb = gtk::Image::builder()
        .pixel_size(28)
        .css_classes(["mini-art"])
        .visible(false)
        .build();
    top.append(&shot_thumb);
    top.append(&mini_art);
    top.append(&compact);
    top.append(&mascot);
    let timer_badge = gtk::Label::builder()
        .css_classes(["timer-badge"])
        .valign(gtk::Align::Center)
        .visible(false)
        .build();
    top.append(&timer_badge);
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
        .icon_name(first_icon(&[
            "view-media-lyrics-symbolic",
            "format-justify-left-symbolic",
        ]))
        .tooltip_text(tr("Lirik", "Lyrics"))
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
    let no_media = gtk::Label::builder()
        .label(tr("Tidak ada media yang diputar", "Nothing playing"))
        .css_classes(["no-media"])
        .build();
    // Shown when more than one player is open; cycles which one the notch controls.
    let player_btn = gtk::Button::builder()
        .css_classes(["player-switch"])
        .halign(gtk::Align::Center)
        .visible(false)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.append(&player_btn);
    body.append(&head);
    body.append(&seek_row);
    body.append(&buttons);
    body.append(&no_media);

    // Home page: editorial asymmetric clock + date on the left, horizontal micro-meters on the right,
    // and quick timer + clipboard pills below.
    let clock = gtk::Label::builder()
        .css_classes(["title", "clock"])
        .xalign(0.0)
        .build();
    let date = gtk::Label::builder()
        .css_classes(["artist"])
        .xalign(0.0)
        .build();
    let clock_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    clock_box.set_valign(gtk::Align::Center);
    clock_box.set_hexpand(true);
    clock_box.append(&clock);
    clock_box.append(&date);

    let stats_box = gtk::Grid::builder()
        .row_spacing(5)
        .column_spacing(8)
        .valign(gtk::Align::Center)
        .build();
    let stat_bars = std::array::from_fn(|_| {
        gtk::ProgressBar::builder()
            .css_classes(["stat-bar"])
            .valign(gtk::Align::Center)
            .build()
    });
    let stat_vals = std::array::from_fn(|_| {
        gtk::Label::builder()
            .css_classes(["stat-val"])
            .xalign(1.0)
            .width_chars(4)
            .build()
    });
    for (row, name) in ["CPU", "RAM", "TMP"].into_iter().enumerate() {
        let lbl = gtk::Label::builder()
            .label(name)
            .css_classes(["stat-lbl"])
            .xalign(0.0)
            .build();
        stats_box.attach(&lbl, 0, row as i32, 1, 1);
        stats_box.attach(&stat_bars[row], 1, row as i32, 1, 1);
        stats_box.attach(&stat_vals[row], 2, row as i32, 1, 1);
    }

    let timer_minus = gtk::Button::builder()
        .label("−")
        .css_classes(["timer-step"])
        .tooltip_text(tr("Kurangi 1 menit", "Minus 1 minute"))
        .build();
    let timer_entry = gtk::Entry::builder()
        .text("25m")
        .width_chars(5)
        .max_width_chars(6)
        .xalign(0.5)
        .css_classes(["timer-input"])
        .tooltip_text(tr(
            "Ketik menit (mis. 10 atau 1:30) lalu Enter",
            "Type minutes (e.g. 10 or 1:30) and press Enter",
        ))
        .build();
    let timer_plus = gtk::Button::builder()
        .label("+")
        .css_classes(["timer-step"])
        .tooltip_text(tr("Tambah 1 menit", "Plus 1 minute"))
        .build();
    let timer_btn = gtk::Button::builder()
        .label("▶")
        .css_classes(["timer-run"])
        .tooltip_text(tr("Mulai / hentikan timer", "Start / stop timer"))
        .build();
    let timer_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    timer_box.add_css_class("timer-group");
    timer_box.set_valign(gtk::Align::Center);
    timer_box.append(&timer_minus);
    timer_box.append(&timer_entry);
    timer_box.append(&timer_plus);
    timer_box.append(&timer_btn);

    let home_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    home_actions.append(&timer_box);
    home_actions.append(&clip_btn);

    let home_head = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    home_head.append(&clock_box);
    home_head.append(&stats_box);
    let home_body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    home_body.append(&home_head);
    home_body.append(&home_actions);
    let clips = gtk::Box::new(gtk::Orientation::Vertical, 4);
    clips.set_width_request(260);
    clips.set_valign(gtk::Align::Start);
    // Full lyrics sit to the right of the player when the lyrics button is on.
    let lyrics_list = gtk::ListBox::new();
    lyrics_list.set_selection_mode(gtk::SelectionMode::None);
    let lyrics_scroll = gtk::ScrolledWindow::builder()
        .child(&lyrics_list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        // min-content-width is ignored without a horizontal scrollbar, so size it directly.
        .width_request(260)
        .height_request(150)
        .margin_start(16)
        .build();
    let lyrics_rev = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .transition_duration(220)
        .child(&lyrics_scroll)
        .visible(false)
        .build();
    lyrics_rev.connect_child_revealed_notify(|r| {
        if !r.is_child_revealed() && !r.reveals_child() {
            r.set_visible(false);
        }
    });
    // Downloads page: everything in progress, then the newest files in ~/Downloads.
    let section = |text: &str| {
        gtk::Label::builder()
            .label(text)
            .css_classes(["section"])
            .xalign(0.0)
            .build()
    };
    let dl_active = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let dl_recent = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let open_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    open_row.set_halign(gtk::Align::Center);
    open_row.append(&gtk::Image::from_icon_name(first_icon(&[
        "folder-open-symbolic",
        "folder-download-symbolic",
    ])));
    open_row.append(&gtk::Label::new(Some(tr(
        "Buka folder Download",
        "Open Downloads folder",
    ))));
    let open_folder = gtk::Button::builder()
        .child(&open_row)
        .css_classes(["clip"])
        .build();
    open_folder.connect_clicked(|_| {
        let _ = gio::AppInfo::launch_default_for_uri(
            &gio::File::for_path(downloads_dir()).uri(),
            None::<&gio::AppLaunchContext>,
        );
    });
    let downloads_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    downloads_page.set_width_request(300);
    downloads_page.append(&section(tr("Sedang diunduh", "Downloading")));
    downloads_page.append(&dl_active);
    downloads_page.append(&section(tr("Terbaru di Downloads", "Latest in Downloads")));
    downloads_page.append(&dl_recent);
    downloads_page.append(&open_folder);
    let media_page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    media_page.append(&body);
    media_page.append(&lyrics_rev);
    // QR of one clipboard item, replacing the list until clicked.
    let qr_pic = gtk::Picture::builder()
        .can_shrink(false)
        .halign(gtk::Align::Center)
        .build();
    let qr_view = gtk::Box::new(gtk::Orientation::Vertical, 6);
    qr_view.append(&qr_pic);
    qr_view.append(
        &gtk::Label::builder()
            .label(tr(
                "Pindai dengan HP · klik untuk kembali",
                "Scan with your phone · click to go back",
            ))
            .css_classes(["time"])
            .build(),
    );
    let clip_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::SlideLeftRight)
        .transition_duration(180)
        .interpolate_size(true)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .margin_start(16)
        .build();
    clip_stack.add_named(&clips, Some("list"));
    clip_stack.add_named(&qr_view, Some("qr"));
    let back = gtk::GestureClick::new();
    let cs = clip_stack.clone();
    back.connect_released(move |_, _, _, _| cs.set_visible_child_name("list"));
    qr_view.add_controller(back);
    let clips_rev = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .transition_duration(220)
        .child(&clip_stack)
        .visible(false)
        .build();
    clips_rev.connect_child_revealed_notify(|r| {
        if !r.is_child_revealed() && !r.reveals_child() {
            r.set_visible(false);
        }
    });
    let home_page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    home_page.append(&home_body);
    home_page.append(&clips_rev);
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
    // Mixer page: output volume and per-app sliders, filled when shown (see mixer.rs).
    let mixer_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    mixer_box.set_width_request(340);
    if cfg().mixer {
        pages.add_named(&mixer_box, Some("mixer"));
    }
    // Screenshots page: thumbnail row, filled by screenshots.rs.
    let shots_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let shots_page = gtk::Box::new(gtk::Orientation::Vertical, 6);
    shots_page.append(
        &gtk::Label::builder()
            .label("Screenshots")
            .css_classes(["section"])
            .xalign(0.0)
            .build(),
    );
    shots_page.append(&shots_row);
    shots_page.append(
        &gtk::Label::builder()
            .label(tr(
                "Seret ke aplikasi lain · klik untuk membuka",
                "Drag into another app · click to open",
            ))
            .css_classes(["time"])
            .xalign(0.0)
            .build(),
    );
    if cfg().downloads {
        pages.add_named(&downloads_page, Some("downloads"));
    }
    if cfg().screenshots {
        pages.add_named(&shots_page, Some("screenshots"));
    }
    let nav_bar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    nav_bar.add_css_class("page-nav");
    nav_bar.set_halign(gtk::Align::Center);
    let page_label = |id: &str| match id {
        "home" => tr("Home", "Home"),
        "media" => tr("Media", "Media"),
        "mixer" => tr("Mixer", "Mixer"),
        "downloads" => tr("Unduh", "Downloads"),
        "screenshots" => tr("Shots", "Shots"),
        _ => "",
    };
    let page_pills: Vec<(&'static str, gtk::Button)> = page_order()
        .into_iter()
        .map(|id| {
            let b = gtk::Button::builder()
                .label(page_label(id))
                .css_classes(["page-pill"])
                .build();
            nav_bar.append(&b);
            (id, b)
        })
        .collect();
    let pager = gtk::Box::new(gtk::Orientation::Vertical, 4);
    pager.set_margin_top(4);
    pager.append(&nav_bar);
    pager.append(&pages);
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
    // The arrow tells it apart from the song, which uses the same accent color.
    let dl_bar = gtk::ProgressBar::builder()
        .css_classes(["dl"])
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    let dl_row = gtk::Box::builder()
        .spacing(6)
        .css_classes(["dl-row"])
        .visible(false)
        .build();
    let dl_icon = first_icon(&["download-symbolic", "folder-download-symbolic"]);
    dl_row.append(
        &gtk::Image::builder()
            .icon_name(dl_icon)
            .pixel_size(12)
            .build(),
    );
    dl_row.append(&dl_bar);
    let lyric_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    lyric_box.append(&lyric);
    lyric_box.append(&lyric_rom);
    let lyric_rev = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(180)
        .child(&lyric_box)
        .visible(false)
        .build();
    lyric_rev.connect_child_revealed_notify(|r| {
        if !r.is_child_revealed() && !r.reveals_child() {
            r.set_visible(false);
        }
    });
    notch.append(&dl_row);
    notch.append(&expanded);
    notch.append(&lyric_rev);
    let song_bar = gtk::ProgressBar::builder()
        .css_classes(["song-trace"])
        .hexpand(true)
        .visible(false)
        .build();
    notch.append(&song_bar);
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
        lyric_rev,
        expanded,
        title,
        artist,
        play,
        buttons,
        mascot,
        lyrics_btn,
        lyrics_list,
        lyrics_scroll,
        lyrics_rev,
        lyric_rom,
        mic,
        cam,
        share,
        stats_box,
        stat_bars,
        stat_vals,
        timer_entry,
        timer_btn,
        timer_badge,
        song_bar,
        clips,
        clip_stack,
        clips_rev,
        clip_btn,
        accent: gtk::CssProvider::new(),
        wall: gtk::CssProvider::new(),
        dl_bar,
        dl_row,
        pages,
        page_pills,
        clock,
        date,
        no_media,
        head,
        dl_active,
        dl_recent,
        player_btn,
        mixer_box,
        shots_row,
        shot_thumb,
        qr_pic,
        pager,
        top_row,
        css: css.clone(),
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
        timer_mins: 25,
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
    ui.lyrics_rev.connect_child_revealed_notify(move |r| {
        if r.is_child_revealed() {
            center_lyric(&u, s.borrow().lyric_idx, false);
        }
    });
    let (u, s) = (ui.clone(), state.clone());
    ui.lyrics_btn
        .connect_toggled(move |_| refresh_compact(&u, &s.borrow()));
    let (u, s) = (ui.clone(), state.clone());
    ui.clip_btn
        .connect_toggled(move |_| refresh_compact(&u, &s.borrow()));
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
    if cfg().mixer {
        spawn_mixer_watch(&ui);
    }
    if cfg().screenshots {
        spawn_screenshot_watch(&ui, &state);
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
                    let _ = if next {
                        p.next().await
                    } else {
                        p.previous().await
                    };
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
    ui.pages
        .connect_visible_child_name_notify(move |p| match p.visible_child_name().as_deref() {
            Some("downloads") => fill_downloads_page(&u, &s.borrow()),
            Some("mixer") => fill_mixer_page(&u),
            _ => {}
        });
    // Switch to the next open player and keep it until it closes.
    let s = state.clone();
    ui.player_btn.connect_clicked(move |_| {
        let mut st = s.borrow_mut();
        let current = st
            .player
            .as_ref()
            .map(|p| p.inner().destination().to_string());
        let i = st
            .players
            .iter()
            .position(|(o, _)| Some(o) == current.as_ref())
            .map_or(0, |i| i + 1);
        if let Some((owner, _)) = st.players.get(i % st.players.len().max(1)).cloned() {
            st.pinned = Some(owner);
            if let Some(tx) = &st.repick {
                let _ = tx.try_send(());
            }
        }
    });
    // Top-edge page pills switch directly on click, or step through pages on scroll.
    for (id, btn) in &ui.page_pills {
        let (u, s, id) = (ui.clone(), state.clone(), *id);
        btn.connect_clicked(move |_| {
            u.pages.set_visible_child_name(id);
            refresh_compact(&u, &s.borrow());
        });
    }
    let nav_scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    let (u, s) = (ui.clone(), state.clone());
    nav_scroll.connect_scroll(move |_, dx, dy| {
        let delta = if dx.abs() > dy.abs() { dx } else { dy };
        if delta.abs() >= 0.5 {
            let order = page_order();
            let i = order
                .iter()
                .position(|p| Some(*p) == u.pages.visible_child_name().as_deref())
                .unwrap_or(0) as i32;
            let step = if delta > 0.0 { 1 } else { -1 };
            if let Some(page) = order.get((i + step).clamp(0, order.len() as i32 - 1) as usize) {
                u.pages.set_visible_child_name(page);
                refresh_compact(&u, &s.borrow());
            }
        }
        glib::Propagation::Stop
    });
    nav_bar.add_controller(nav_scroll);

    // Quick timer: − / + buttons, direct text entry (e.g. "10" or "1:30" + Enter), or smooth scroll.
    let step_timer = |u: &Ui, s: &Shared, w: &gtk::ApplicationWindow, delta_mins: i32| {
        let mut st = s.borrow_mut();
        st.editing_timer = false;
        w.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        if st.timer_rem > 0 {
            let next = (st.timer_rem as i32 + delta_mins * 60).clamp(10, 180 * 60) as u32;
            st.timer_rem = next;
            st.timer_mins = next.div_ceil(60);
        } else {
            st.timer_mins = (st.timer_mins as i32 + delta_mins).clamp(1, 180) as u32;
        }
        drop(st);
        refresh_compact(u, &s.borrow());
    };
    for (btn, d) in [(&timer_minus, -1), (&timer_plus, 1)] {
        let (u, s, w) = (ui.clone(), state.clone(), window.clone());
        btn.connect_clicked(move |_| step_timer(&u, &s, &w, d));
    }
    // Clicking the timer entry requests exclusive Wayland keyboard focus so typing works in any compositor,
    // strips the "m" unit suffix, and selects the digits so the first keystroke replaces them.
    let entry_click = gtk::GestureClick::new();
    entry_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (u, s, w) = (ui.clone(), state.clone(), window.clone());
    entry_click.connect_pressed(move |_, _, _, _| {
        let already = s.borrow().editing_timer;
        s.borrow_mut().editing_timer = true;
        w.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);
        if !already {
            let clean = u.timer_entry.text().trim_end_matches('m').to_string();
            u.timer_entry.set_text(&clean);
            let e = u.timer_entry.clone();
            glib::idle_add_local_once(move || {
                let _ = e.grab_focus();
                e.select_region(0, -1);
            });
        }
    });
    ui.timer_entry.add_controller(entry_click);
    let (u, s, w) = (ui.clone(), state.clone(), window.clone());
    ui.timer_entry.connect_activate(move |e| {
        {
            let mut st = s.borrow_mut();
            st.editing_timer = false;
            if let Some(secs) = parse_timer_secs(&e.text()) {
                st.timer_rem = secs;
                st.timer_mins = secs.div_ceil(60).max(1);
            }
        }
        w.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        let _ = u.timer_btn.grab_focus();
        refresh_compact(&u, &s.borrow());
    });
    let (u, s, w) = (ui.clone(), state.clone(), window.clone());
    ui.timer_btn.connect_clicked(move |_| {
        {
            let mut st = s.borrow_mut();
            st.editing_timer = false;
            if st.timer_rem > 0 {
                st.timer_rem = 0;
            } else {
                let secs =
                    parse_timer_secs(&u.timer_entry.text()).unwrap_or(st.timer_mins.max(1) * 60);
                st.timer_rem = secs;
                st.timer_mins = secs.div_ceil(60).max(1);
            }
        }
        w.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        refresh_compact(&u, &s.borrow());
    });
    let timer_scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    timer_scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    let timer_acc = Rc::new(std::cell::Cell::new(0.0f64));
    let (u, s, w, ta) = (ui.clone(), state.clone(), window.clone(), timer_acc.clone());
    timer_scroll.connect_scroll(move |_, _, dy| {
        let next = ta.get() + dy;
        if next.abs() >= 0.8 {
            ta.set(0.0);
            step_timer(&u, &s, &w, if next < 0.0 { 1 } else { -1 });
        } else {
            ta.set(next);
        }
        glib::Propagation::Stop
    });
    timer_scroll.connect_scroll_end(move |_| timer_acc.set(0.0));
    timer_box.add_controller(timer_scroll);

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
            u1.pages
                .set_visible_child_full(page, gtk::StackTransitionType::None);
            set_open(&u1, &s1, true);
        }
    });
    let (u2, s2, w2) = (ui.clone(), state.clone(), window.clone());
    hover.connect_leave(move |_| {
        let seq = hover_gen.get() + 1;
        hover_gen.set(seq);
        let (u, s, w, g) = (u2.clone(), s2.clone(), w2.clone(), hover_gen.clone());
        glib::timeout_add_local_once(Duration::from_millis(250), move || {
            if g.get() == seq {
                {
                    let mut st = s.borrow_mut();
                    if st.editing_timer {
                        st.editing_timer = false;
                        if st.timer_rem == 0
                            && let Some(secs) = parse_timer_secs(&u.timer_entry.text())
                        {
                            st.timer_mins = secs.div_ceil(60).max(1);
                        }
                    }
                }
                w.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
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
            u.time.set_label(&format!(
                "{} / {}",
                mmss(secs as i64),
                mmss(u.seek.adjustment().upper() as i64)
            ));
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
            let Some(p) = state.borrow().player.clone() else {
                return;
            };
            glib::spawn_future_local(async move {
                let _ = match action {
                    "prev" => p.previous().await,
                    "next" => p.next().await,
                    _ => p.play_pause().await,
                };
            });
        });
    }

    if cfg().stats {
        update_stats(&ui, &mut state.borrow_mut().cpu_prev);
    }
    refresh_compact(&ui, &state.borrow());
    let (u, s) = (ui.clone(), state.clone());
    glib::timeout_add_seconds_local(1, move || {
        let finished = {
            let mut st = s.borrow_mut();
            if st.timer_rem > 0 {
                st.timer_rem -= 1;
                st.timer_rem == 0
            } else {
                false
            }
        };
        if finished {
            show_alert_for(
                &u,
                &s,
                tr("⏱  Timer selesai!", "⏱  Timer finished!").into(),
                5,
            );
        } else {
            refresh_compact(&u, &s.borrow());
        }
        if u.stats_box.is_visible()
            && u.expanded.reveals_child()
            && u.pages.visible_child_name().as_deref() == Some("home")
        {
            update_stats(&u, &mut s.borrow_mut().cpu_prev);
        }
        glib::ControlFlow::Continue
    });

    // ponytail: MPRIS never signals Position, so poll it while music plays; extrapolate locally if 2 calls/s ever matters.
    let (u, s) = (ui.clone(), state.clone());
    glib::timeout_add_local(Duration::from_millis(500), move || {
        let st = s.borrow();
        if let (true, None, Some(p)) = (st.playing, st.seeking, st.player.clone()) {
            drop(st);
            let (u, s) = (u.clone(), s.clone());
            glib::spawn_future_local(async move {
                if let Ok(us) = p.position().await {
                    let line = current_line(&s.borrow().lyrics, us / 1000).unwrap_or_default();
                    let idx = current_index(&s.borrow().lyrics, us / 1000);
                    let rom = idx.and_then(|i| s.borrow().lyrics_rom.get(i).cloned().flatten());
                    if u.lyric.label() != line && !line.is_empty() {
                        retrigger(&u.lyric, "fade", 250);
                        retrigger(&u.lyric_rom, "fade", 250);
                    }
                    u.lyric.set_label(&line);
                    u.lyric_rom.set_label(rom.as_deref().unwrap_or_default());
                    highlight_lyric(&u, &mut s.borrow_mut(), idx);
                    let secs = us as f64 / 1e6;
                    let upper = u.seek.adjustment().upper().max(1.0);
                    u.seek.set_value(secs);
                    u.song_bar.set_fraction((secs / upper).clamp(0.0, 1.0));
                    u.time.set_label(&format!(
                        "{} / {}",
                        mmss(us / 1_000_000),
                        mmss(upper as i64)
                    ));
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
        spawn_download_watch(&ui, &state);
        let (u, s) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            if let Err(e) = download_loop(u, s).await {
                eprintln!("downloads: {e}");
            }
        });
    }

    #[cfg(debug_assertions)]
    demo(&ui, &state);
    window.present();
}

/// Debug builds only: NUSA_DEMO puts the notch in a state for tests and screenshots,
/// 3 s after start (see tools/shot.sh). Comma-separated:
/// `page=<home|media|mixer|downloads|screenshots>` opens on that page,
/// `lyrics` / `clips` turn on those panels, `mascot` wakes the mascot,
/// `mic` shows the mic dot, `alert=<text>` shows a popup.
#[cfg(debug_assertions)]
fn demo(ui: &Ui, state: &Shared) {
    let Ok(spec) = std::env::var("NUSA_DEMO") else {
        return;
    };
    let (ui, state) = (ui.clone(), state.clone());
    glib::timeout_add_seconds_local_once(3, move || {
        let mut st = state.borrow_mut();
        st.mic_apps = if spec.split(',').any(|o| o == "mic") {
            vec!["Teams".into()]
        } else {
            vec![]
        };
        drop(st);
        for opt in spec.split(',') {
            match opt.split_once('=') {
                Some(("page", page)) => {
                    if page == "home" && cfg().stats {
                        update_stats(&ui, &mut state.borrow_mut().cpu_prev);
                    }
                    ui.pages
                        .set_visible_child_full(page, gtk::StackTransitionType::None);
                    set_open(&ui, &state, true);
                }
                Some(("alert", text)) => show_alert_for(&ui, &state, text.into(), 30),
                Some(("timer", secs)) => {
                    state.borrow_mut().timer_rem = secs.parse().unwrap_or(1500);
                }
                _ if opt == "mascot" => set_mood(&ui, &state, Mood::Awake),
                _ => {}
            }
        }
        // Panels need the open notch, which set_open reveals after its widen step.
        let (u, s, spec) = (ui.clone(), state.clone(), spec.clone());
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            u.lyrics_btn
                .set_active(spec.split(',').any(|o| o == "lyrics"));
            u.clip_btn.set_active(spec.split(',').any(|o| o == "clips"));
            refresh_compact(&u, &s.borrow());
        });
        refresh_compact(&ui, &state.borrow());
    });
}

fn refresh_compact(ui: &Ui, st: &State) {
    let text = if let Some(a) = &st.alert {
        a.clone()
    } else if st.playing {
        format!("♪  {}", st.track)
    } else if let Some((_, label, _)) = &st.download {
        format!("⬇  {label}")
    } else {
        glib::DateTime::now_local()
            .and_then(|d| d.format("%H:%M"))
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    // Fade new text in, except the clock ticking over or download progress ticking up.
    let old = ui.compact.label();
    let is_clock = |t: &str| t.len() == 5 && t.as_bytes()[2] == b':';
    let is_dl = |t: &str| t.starts_with("⬇  ") && st.alert.is_none();
    if old != text && !(is_clock(&old) && is_clock(&text)) && !(is_dl(&old) && is_dl(&text)) {
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
    if full_lyrics {
        ui.lyrics_rev.set_visible(true);
        ui.lyrics_rev.set_reveal_child(true);
    } else {
        ui.lyrics_rev.set_reveal_child(false);
        if !ui.lyrics_rev.is_child_revealed() {
            ui.lyrics_rev.set_visible(false);
        }
    }
    let show_lyric = st.playing
        && !st.lyrics.is_empty()
        && st.alert.is_none()
        && (!open || (on_media && !full_lyrics));
    ui.lyric.set_visible(true);
    ui.lyric_rom
        .set_visible(st.lyrics_rom.iter().any(Option::is_some));
    if show_lyric {
        ui.lyric_rev.set_visible(true);
        ui.lyric_rev.set_reveal_child(true);
    } else {
        ui.lyric_rev.set_reveal_child(false);
        if !ui.lyric_rev.is_child_revealed() {
            ui.lyric_rev.set_visible(false);
        }
    }
    ui.stats_box.set_visible(cfg().stats);
    let has_clips = ui.clips.first_child().is_some();
    ui.clip_btn.set_visible(has_clips);
    let show_clips = open && !on_media && has_clips && ui.clip_btn.is_active();
    if show_clips {
        ui.clips_rev.set_visible(true);
        ui.clips_rev.set_reveal_child(true);
    } else {
        ui.clip_stack.set_visible_child_name("list");
        ui.clips_rev.set_reveal_child(false);
        if !ui.clips_rev.is_child_revealed() {
            ui.clips_rev.set_visible(false);
        }
    }
    ui.shot_thumb.set_paintable(st.alert_thumb.as_ref());
    ui.shot_thumb
        .set_visible(st.alert.is_some() && st.alert_thumb.is_some());
    for (id, btn) in &ui.page_pills {
        if page.as_deref() == Some(*id) {
            btn.add_css_class("active");
        } else {
            btn.remove_css_class("active");
        }
    }
    if st.timer_rem > 0 {
        let t = mmss(st.timer_rem as i64);
        if !st.editing_timer && ui.timer_entry.text() != t {
            ui.timer_entry.set_text(&t);
        }
        ui.timer_btn.set_label("■");
        ui.timer_btn.add_css_class("running");
        ui.timer_badge.set_label(&format!("⏱ {t}"));
        ui.timer_badge.set_visible(true);
    } else {
        let want = format!("{}m", st.timer_mins.max(1));
        if !st.editing_timer && ui.timer_entry.text() != want {
            ui.timer_entry.set_text(&want);
        }
        ui.timer_btn.set_label("▶");
        ui.timer_btn.remove_css_class("running");
        ui.timer_badge.set_visible(false);
    }
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
    let show_compact = st.alert.is_some() || (!open && (!st.minimal || st.download.is_some()));
    ui.top_row.set_reveal_child(show_compact);
    // Items inside the row keep their visibility while it slides away, so the slide has
    // something to animate; the row's revealer alone decides whether it shows.
    ui.mic.set_visible(!st.mic_apps.is_empty());
    ui.cam.set_visible(!st.cam_apps.is_empty() || st.cam_pw);
    ui.share.set_visible(st.sharing);
    // The mascot stands in for the clock (or the song title) once you have been away a while.
    let mascot = st.mood != Mood::None && st.alert.is_none() && st.download.is_none();
    ui.dl_row.set_visible(!open && st.download.is_some());
    if let Some((_, _, p)) = &st.download {
        ui.dl_bar.set_fraction(*p);
    }
    ui.mascot.set_visible(mascot);
    ui.compact.set_visible(!mascot);
    let media = st.playing && st.alert.is_none();
    ui.eq.set_visible(media);
    ui.mini_art
        .set_visible(media && !mascot && ui.mini_art.paintable().is_some());
    if ui.expanded.reveals_child() {
        ui.notch.add_css_class("open");
    } else {
        ui.notch.remove_css_class("open");
    }
    let thin = !open && !show_compact && !show_lyric;
    ui.song_bar
        .set_visible(!open && media && !thin && ui.seek.adjustment().upper() > 1.0);
    // While music plays the closed notch keeps one width, so long titles and each new lyric
    // line don't make it grow and shrink. The labels then ellipsize to that width.
    let fixed = media && !thin && !ui.want_open.get();
    for (class, on) in [("thin", thin), ("music", fixed)] {
        if on {
            ui.notch.add_css_class(class);
        } else {
            ui.notch.remove_css_class(class);
        }
    }
    for (label, chars) in [(&ui.compact, 32), (&ui.lyric, 40), (&ui.lyric_rom, 48)] {
        label.set_max_width_chars(if fixed { 1 } else { chars });
    }
    // Player switch: name of the one in control, only when there is a choice.
    let current = st
        .player
        .as_ref()
        .map(|p| p.inner().destination().to_string());
    let name = st
        .players
        .iter()
        .find(|(o, _)| Some(o) == current.as_ref())
        .map(|(_, n)| n.as_str())
        .unwrap_or("");
    ui.player_btn.set_visible(st.players.len() > 1);
    ui.player_btn.set_label(&format!("{name}  ⇄"));
    // Media page: the player, or the "nothing playing" note.
    ui.head.set_visible(has_track);
    ui.buttons.set_visible(has_track);
    ui.no_media.set_visible(!has_track);
    if !has_track {
        ui.seek.parent().unwrap().set_visible(false);
    }
    // Home page clock.
    let now = glib::DateTime::now_local().ok();
    ui.clock.set_label(
        &now.as_ref()
            .and_then(|d| d.format("%H:%M").ok())
            .unwrap_or_default(),
    );
    ui.date
        .set_label(&now.as_ref().map(tanggal).unwrap_or_default());
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
        ui.width_css
            .load_from_string(&format!(".notch {{ min-width: {natural}px; }}"));
        let (u, s) = (ui.clone(), state.clone());
        glib::timeout_add_local_once(Duration::from_millis(170), move || {
            if u.anim.get() == seq {
                u.pager.set_visible(true);
                u.expanded.set_reveal_child(true);
                u.width_css.load_from_string("");
                refresh_compact(&u, &s.borrow());
            }
        });
    } else {
        if ui.pager.is_visible() {
            let (_, natural, _, _) = ui.pager.measure(gtk::Orientation::Horizontal, -1);
            ui.width_css
                .load_from_string(&format!(".notch {{ min-width: {natural}px; }}"));
        }
        ui.expanded.set_reveal_child(false);
        ui.clip_btn.set_active(false);
        ui.clip_stack.set_visible_child_name("list");
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
    if cfg().mixer {
        order.push("mixer");
    }
    if cfg().downloads {
        order.push("downloads");
    }
    if cfg().screenshots {
        order.push("screenshots");
    }
    order
}

/// The first of `names` anywhere in the icon theme or the themes it inherits.
/// (A multi-name GIcon would stop at the first theme holding any of them, often the fallback.)
fn first_icon(names: &[&'static str]) -> &'static str {
    let theme = gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap());
    names
        .iter()
        .copied()
        .find(|n| theme.has_icon(n))
        .unwrap_or(names[names.len() - 1])
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
        st.alert_thumb = None;
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

const HARI: [&str; 7] = [
    "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu",
];
const BULAN: [&str; 12] = [
    "Januari",
    "Februari",
    "Maret",
    "April",
    "Mei",
    "Juni",
    "Juli",
    "Agustus",
    "September",
    "Oktober",
    "November",
    "Desember",
];
const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "Senin, 26 Oktober 2026" or "Monday, 26 October 2026", independent of the system locale.
fn tanggal(d: &glib::DateTime) -> String {
    format_date(d, cfg().english)
}

fn format_date(d: &glib::DateTime, english: bool) -> String {
    let (days, months) = if english {
        (DAYS, MONTHS)
    } else {
        (HARI, BULAN)
    };
    let (day, month) = (
        days[d.day_of_week() as usize - 1],
        months[d.month() as usize - 1],
    );
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

/// Parse a timer input like "10", "10m", "90s", or "1:30" into seconds (clamped 5s..=3h).
fn parse_timer_secs(raw: &str) -> Option<u32> {
    let s = raw.trim().to_ascii_lowercase();
    let secs = if let Some((m, sec)) = s.split_once(':') {
        m.trim().parse::<u32>().ok()? * 60 + sec.trim().parse::<u32>().ok()?
    } else if let Some(sec) = s.strip_suffix('s') {
        sec.trim().parse::<u32>().ok()?
    } else {
        s.trim_end_matches('m').trim().parse::<u32>().ok()? * 60
    };
    (secs > 0).then(|| secs.clamp(5, 180 * 60))
}

#[test]
fn timer_input_parsing() {
    assert_eq!(parse_timer_secs("25"), Some(1500));
    assert_eq!(parse_timer_secs("10m"), Some(600));
    assert_eq!(parse_timer_secs("45s"), Some(45));
    assert_eq!(parse_timer_secs("1:30"), Some(90));
    assert_eq!(parse_timer_secs("0"), None);
}

#[test]
fn tanggal_indonesia() {
    let d = glib::DateTime::from_local(2026, 10, 26, 9, 0, 0.0).unwrap();
    assert_eq!(format_date(&d, false), "Senin, 26 Oktober 2026");
    assert_eq!(format_date(&d, true), "Monday, 26 October 2026");
    let d = glib::DateTime::from_local(2026, 2, 1, 9, 0, 0.0).unwrap();
    assert_eq!(format_date(&d, false), "Minggu, 1 Februari 2026");
}
