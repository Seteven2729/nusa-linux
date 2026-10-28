//! System events shown in the notch: power, network, Bluetooth, downloads, mic, clipboard, phone, stats, wallpaper.

use crate::*;

/// Partial file extensions browsers use while a download is still being written.
const PARTIAL: [&str; 4] = [".part", ".crdownload", ".download", ".tmp"];

fn is_partial(name: &str) -> bool {
    PARTIAL.iter().any(|p| name.ends_with(p))
}

fn strip_partial(name: &str) -> Option<&str> {
    PARTIAL.iter().find_map(|p| name.strip_suffix(p))
}

/// True when `dir` still has an in-progress partial file for `name` (Firefox/Zen create a
/// 0-byte `<name>` placeholder alongside `<name>.part` when a download starts).
fn has_partial(dir: &std::path::Path, name: &str) -> bool {
    PARTIAL.iter().any(|p| dir.join(format!("{name}{p}")).exists())
}

fn is_browser(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    [
        "firefox", "zen", "chrom", "brave", "vivaldi", "edge", "opera", "librewolf", "waterfox",
        "floorp", "thorium", "epiphany", "falkon",
    ]
    .iter()
    .any(|b| s.contains(b))
}

/// Smooth progress curve (8%..92%) for `.part` downloads whose final size is not known.
fn part_fraction(bytes: u64) -> f64 {
    (1.0 - 1.0 / (1.0 + bytes as f64 / (20.0 * 1048576.0))).clamp(0.08, 0.92)
}

/// Download progress that apps (browsers, file managers) publish through the Unity
/// LauncherEntry API, the same signal Plasma's task manager reads for its progress bars.
pub(crate) async fn download_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("com.canonical.Unity.LauncherEntry")
        .member("Update")
        .build()?;
    let mut stream = MessageStream::for_match_rule(rule, &conn, None).await?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
    // Updates only carry the keys that changed, so keep the last known (progress, visible) per app.
    let mut entries: HashMap<String, (f64, bool)> = HashMap::new();
    // Music players (Strawberry) report song position through the same API; that's not a download.
    // Browsers also own MPRIS names when a tab plays video, so never filter out a browser here.
    let mut is_player: HashMap<String, bool> = HashMap::new();
    while let Some(Ok(msg)) = stream.next().await {
        let Some(sender) = msg.header().sender().map(|s| s.to_string()) else {
            continue;
        };
        let Ok((uri, props)) = msg.body().deserialize::<(String, HashMap<String, OwnedVariant>)>() else {
            continue;
        };
        if !is_browser(&uri) {
            if !is_player.contains_key(&sender) {
                let mut player = false;
                for name in dbus.list_names().await.unwrap_or_default() {
                    if name.starts_with("org.mpris.MediaPlayer2.")
                        && !is_browser(&name)
                        && dbus.get_name_owner(name.as_ref()).await.is_ok_and(|o| o.as_str() == sender)
                    {
                        player = true;
                    }
                }
                is_player.insert(sender.clone(), player);
            }
            if is_player[&sender] {
                continue;
            }
        }
        let e = entries.entry(uri.clone()).or_insert((0.0, false));
        let was_visible = e.1;
        if let Some(p) = props.get("progress").and_then(|v| f64::try_from(v.try_clone().ok()?).ok()) {
            e.0 = p.clamp(0.0, 1.0);
        }
        if let Some(v) = props.get("progress-visible").and_then(|v| bool::try_from(v.try_clone().ok()?).ok()) {
            e.1 = v;
        }
        let (progress, visible) = *e;
        let app = app_name(&uri);
        let pct = |p: f64| format!("{}%", (p * 100.0).round());
        let mut active: Vec<(String, String, f64)> = entries
            .iter()
            .filter(|(_, (_, v))| *v)
            .map(|(u, (p, _))| (app_name(u), pct(*p), *p))
            .collect();
        active.sort_by(|a, b| a.0.cmp(&b.0));
        state.borrow_mut().downloads = active;
        if visible {
            // Most recently updated download wins when several run at once.
            state.borrow_mut().download = Some((app, pct(progress), progress));
            refresh_compact(&ui, &state.borrow());
        } else if was_visible {
            let showing = state.borrow().download.as_ref().is_some_and(|(a, _, _)| *a == app);
            if showing {
                state.borrow_mut().download = None;
            }
            show_alert(
                &ui,
                &state,
                format!("{} · {app}", tr("⬇  Download selesai", "⬇  Download finished")),
            );
        }
        if ui.pages.visible_child_name().as_deref() == Some("downloads") {
            fill_downloads_page(&ui, &state.borrow());
        }
    }
    Ok(())
}

/// Watch `~/Downloads` with inotify for browsers (Firefox, Zen) and tools that don't emit
/// `com.canonical.Unity.LauncherEntry` on Wayland: track growing `.part`/`.crdownload` files
/// while they are written, and alert when the final file lands.
pub(crate) fn spawn_download_watch(ui: &Ui, state: &Shared) {
    enum DlEvent {
        Part(String),
        Done(String),
    }
    let dir = downloads_dir();
    let (tx, rx) = async_channel::unbounded::<DlEvent>();
    let watch_dir = dir.clone();
    std::thread::spawn(move || {
        let _ = std::fs::create_dir_all(&watch_dir);
        // SAFETY: plain inotify syscalls on a descriptor owned by this thread.
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        let Ok(cdir) = std::ffi::CString::new(watch_dir.as_os_str().as_encoded_bytes()) else {
            return;
        };
        let mask = libc::IN_CREATE
            | libc::IN_MODIFY
            | libc::IN_CLOSE_WRITE
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO
            | libc::IN_DELETE;
        if fd < 0 || unsafe { libc::inotify_add_watch(fd, cdir.as_ptr(), mask) } < 0 {
            return;
        }
        let mut buf = [0u8; 4096];
        let mut last_part = std::time::Instant::now() - Duration::from_secs(1);
        loop {
            let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                return;
            }
            let mut off = 0;
            while off + 16 <= n as usize {
                let emask = u32::from_ne_bytes(buf[off + 4..off + 8].try_into().unwrap());
                let len = u32::from_ne_bytes(buf[off + 12..off + 16].try_into().unwrap()) as usize;
                let name = &buf[off + 16..off + 16 + len];
                let name = String::from_utf8_lossy(name.split(|b| *b == 0).next().unwrap_or_default()).into_owned();
                off += 16 + len;
                if name.is_empty() || name.starts_with('.') {
                    continue;
                }
                if let Some(target) = strip_partial(&name) {
                    if target.is_empty() || target.starts_with('.') {
                        continue;
                    }
                    let removed = (emask & (libc::IN_DELETE | libc::IN_MOVED_FROM)) != 0;
                    if removed || last_part.elapsed() >= Duration::from_millis(150) {
                        last_part = std::time::Instant::now();
                        if tx.send_blocking(DlEvent::Part(name)).is_err() {
                            return;
                        }
                    }
                } else if (emask & (libc::IN_MOVED_TO | libc::IN_CLOSE_WRITE)) != 0
                    && tx.send_blocking(DlEvent::Done(name)).is_err()
                {
                    return;
                }
            }
        }
    });
    let (ui, state) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        let mut active_parts: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut last_done: Option<(String, std::time::Instant)> = None;
        loop {
            let ev = if active_parts.is_empty() {
                let Ok(ev) = rx.recv().await else { return };
                Some(ev)
            } else {
                match futures_util::future::select(
                    std::pin::pin!(rx.recv()),
                    std::pin::pin!(glib::timeout_future_seconds(2)),
                )
                .await
                {
                    futures_util::future::Either::Left((Ok(ev), _)) => Some(ev),
                    futures_util::future::Either::Left((Err(_), _)) => return,
                    futures_util::future::Either::Right(_) => None,
                }
            };
            let mut finished = None;
            match ev {
                Some(DlEvent::Part(part)) => {
                    active_parts.insert(part);
                }
                Some(DlEvent::Done(name)) => {
                    for ext in PARTIAL {
                        active_parts.remove(&format!("{name}{ext}"));
                    }
                    if let Ok(meta) = dir.join(&name).metadata()
                        && meta.is_file()
                        && meta.len() > 0
                        && !has_partial(&dir, &name)
                    {
                        let dup = last_done
                            .as_ref()
                            .is_some_and(|(n, t)| *n == name && t.elapsed() < Duration::from_secs(1));
                        if !dup {
                            last_done = Some((name.clone(), std::time::Instant::now()));
                            finished = Some(name);
                        }
                    }
                }
                None => {}
            }
            let now = std::time::SystemTime::now();
            let mut live: Vec<(String, u64, std::time::SystemTime)> = Vec::new();
            active_parts.retain(|part| {
                let Some(target) = strip_partial(part) else {
                    return false;
                };
                let Ok(meta) = dir.join(part).metadata() else {
                    return false;
                };
                let Ok(modified) = meta.modified() else {
                    return false;
                };
                if !meta.is_file()
                    || now.duration_since(modified).unwrap_or_default() > Duration::from_secs(12)
                {
                    return false;
                }
                live.push((target.to_owned(), meta.len(), modified));
                true
            });
            live.sort_by(|a, b| b.2.cmp(&a.2));
            let latest = live.first().map(|(name, bytes, _)| {
                (name.clone(), human_size(*bytes, cfg().english), part_fraction(*bytes))
            });
            let mut active: Vec<(String, String, f64)> = live
                .into_iter()
                .map(|(name, bytes, _)| (name, human_size(bytes, cfg().english), part_fraction(bytes)))
                .collect();
            active.sort_by(|a, b| a.0.cmp(&b.0));
            {
                let mut st = state.borrow_mut();
                st.downloads = active;
                st.download = latest;
            }
            if let Some(name) = finished {
                show_alert(
                    &ui,
                    &state,
                    format!("{} · {name}", tr("⬇  Download selesai", "⬇  Download finished")),
                );
            } else {
                refresh_compact(&ui, &state.borrow());
            }
            if ui.pages.visible_child_name().as_deref() == Some("downloads") {
                fill_downloads_page(&ui, &state.borrow());
            }
        }
    });
}

/// "application://org.mozilla.firefox.desktop" -> "firefox", "application://google-chrome.desktop" -> "google-chrome".
pub(crate) fn downloads_dir() -> PathBuf {
    glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(|| glib::home_dir().join("Downloads"))
}

/// Rebuild the downloads page: a progress row per active download, then the newest files.
pub(crate) fn fill_downloads_page(ui: &Ui, st: &State) {
    for b in [&ui.dl_active, &ui.dl_recent] {
        while let Some(c) = b.first_child() {
            b.remove(&c);
        }
    }
    let muted = |text: &str| gtk::Label::builder().label(text).css_classes(["time"]).xalign(0.0).build();
    if st.downloads.is_empty() {
        ui.dl_active.append(&muted(tr("Tidak ada download aktif", "No active downloads")));
    }
    for (name, label, p) in &st.downloads {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(
            &gtk::Label::builder()
                .label(name.as_str())
                .hexpand(true)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::Middle)
                .max_width_chars(24)
                .build(),
        );
        row.append(&muted(label));
        ui.dl_active.append(&row);
        ui.dl_active
            .append(&gtk::ProgressBar::builder().fraction(*p).css_classes(["dl"]).build());
    }
    let files = recent_downloads(&downloads_dir(), 4);
    if files.is_empty() {
        ui.dl_recent
            .append(&muted(tr("Folder Download kosong", "Downloads folder is empty")));
    }
    let now = std::time::SystemTime::now();
    for (path, size, modified) in files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let age = now.duration_since(modified).unwrap_or_default().as_secs();
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(
            &gtk::Label::builder()
                .label(format!("📄  {name}"))
                .hexpand(true)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::Middle)
                .max_width_chars(24)
                .build(),
        );
        row.append(&muted(&format!(
            "{} · {}",
            human_size(size, cfg().english),
            ago(age, cfg().english)
        )));
        let btn = gtk::Button::builder().child(&row).css_classes(["clip"]).build();
        btn.connect_clicked(move |_| {
            let _ = gio::AppInfo::launch_default_for_uri(&gio::File::for_path(&path).uri(), None::<&gio::AppLaunchContext>);
        });
        ui.dl_recent.append(&btn);
    }
}

/// Newest finished files in `dir`: (path, size, modified). Skips folders, hidden files and
/// browsers' in-progress files (including Firefox/Zen 0-byte placeholders alongside `.part`).
fn recent_downloads(dir: &std::path::Path, n: usize) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !name.starts_with('.') && !is_partial(&name) && !has_partial(dir, &name)
        })
        .filter_map(|e| {
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            Some((e.path(), meta.len(), meta.modified().ok()?))
        })
        .collect();
    files.sort_by(|a, b| b.2.cmp(&a.2));
    files.truncate(n);
    files
}

/// "2,4 MB" (Indonesian decimal comma) or "2.4 MB".
fn human_size(bytes: u64, english: bool) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < units.len() {
        v /= 1024.0;
        u += 1;
    }
    let s = if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{:.1} {}", v, units[u])
    };
    if english { s } else { s.replace('.', ",") }
}

/// "baru saja", "5 mnt", "3 jam", "2 hari" (or "just now", "5 min", "3 h", "2 d").
fn ago(secs: u64, english: bool) -> String {
    let (now, min, hour, day) = if english {
        ("just now", "min", "h", "d")
    } else {
        ("baru saja", "mnt", "jam", "hari")
    };
    match secs {
        0..60 => now.into(),
        60..3600 => format!("{} {min}", secs / 60),
        3600..86400 => format!("{} {hour}", secs / 3600),
        _ => format!("{} {day}", secs / 86400),
    }
}

#[test]
fn download_page_formatting() {
    assert_eq!(human_size(512, false), "512 B");
    assert_eq!(human_size(2_500_000, false), "2,4 MB");
    assert_eq!(human_size(2_500_000, true), "2.4 MB");
    assert_eq!(ago(30, false), "baru saja");
    assert_eq!(ago(300, false), "5 mnt");
    assert_eq!(ago(7200, false), "2 jam");
    assert_eq!(ago(200_000, false), "2 hari");
    assert_eq!(ago(300, true), "5 min");

    let dir = std::env::temp_dir().join(format!("nusa-dl-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, delay) in [
        ("old.pdf", 0),
        ("movie.mkv", 20),
        ("movie.mkv.part", 5),
        ("new.zip", 20),
        (".hidden", 20),
    ] {
        std::thread::sleep(Duration::from_millis(delay));
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    let names: Vec<_> = recent_downloads(&dir, 4)
        .iter()
        .map(|f| f.0.file_name().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        ["new.zip", "old.pdf"],
        "newest first, partial, placeholder and hidden files skipped"
    );
    assert_eq!(strip_partial("zen.tar.xz.part"), Some("zen.tar.xz"));
    assert!(is_browser("application://zen.desktop"));
    assert!(!is_browser("application://org.strawberrymusicplayer.strawberry.desktop"));
    assert!(part_fraction(0) >= 0.08 && part_fraction(500_000_000) <= 0.92);
    std::fs::remove_dir_all(dir).unwrap();
}

pub(crate) fn app_name(uri: &str) -> String {
    let id = uri.trim_start_matches("application://").trim_end_matches(".desktop");
    id.rsplit('.').next().unwrap_or(id).to_string()
}

pub(crate) async fn battery_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
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
            1 => format!("{}  {pct}%", tr("⚡ Mengisi daya", "⚡ Charging")),
            2 => format!("{}  {pct}%", tr("🔋 Pakai baterai", "🔋 On battery")),
            4 => tr("⚡ Baterai penuh", "⚡ Fully charged").into(),
            _ => continue,
        };
        show_alert(&ui, &state, text);
    }
    Ok(())
}

pub(crate) async fn network_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
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
            let icon = if active.kind().await.as_deref() == Ok("802-11-wireless") {
                "📶"
            } else {
                "🔌"
            };
            show_alert(&ui, &state, format!("{icon}  {} · {name}", tr("Terhubung", "Connected")));
        } else if now <= 20 && online {
            online = false;
            show_alert(&ui, &state, tr("⚠  Internet terputus", "⚠  Internet disconnected").into());
        }
    }
    Ok(())
}

pub(crate) async fn bluetooth_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
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
        let Ok((_, changed, _)) = msg.body().deserialize::<(String, HashMap<String, OwnedVariant>, Vec<String>)>() else {
            continue;
        };
        let Some(connected) = changed.get("Connected").and_then(|v| bool::try_from(v.try_clone().ok()?).ok()) else {
            continue;
        };
        let Some(path) = msg.header().path().map(|p| p.to_owned()) else {
            continue;
        };
        let dev = BtDeviceProxy::builder(&conn).path(path.clone()).build().await?;
        // Only audio gear (headphones, earbuds, speakers); mice and keyboards connect too often to be worth a popup.
        if !dev.icon().await.unwrap_or_default().starts_with("audio") {
            continue;
        }
        let name = dev.alias().await.unwrap_or_default();
        if !connected {
            show_alert(&ui, &state, format!("🎧  {name} {}", tr("terputus", "disconnected")));
            continue;
        }
        show_alert(&ui, &state, format!("🎧  {name}"));
        // BlueZ publishes the battery a moment after connecting, if the device reports one.
        let (ui, state, conn) = (ui.clone(), state.clone(), conn.clone());
        glib::spawn_future_local(async move {
            glib::timeout_future_seconds(2).await;
            let Ok(bat) = BtBatteryProxy::builder(&conn).path(path).build().await else {
                return;
            };
            if let Ok(pct) = bat.percentage().await {
                show_alert(&ui, &state, format!("🎧  {name} · {pct}%"));
            }
        });
    }
    Ok(())
}

pub(crate) fn change_volume(ui: &Ui, state: &Shared, step: &'static str) {
    let (ui, state) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        let out = gio::spawn_blocking(move || {
            let sink = "@DEFAULT_AUDIO_SINK@";
            std::process::Command::new("wpctl")
                .args(["set-volume", "-l", "1.0", sink, step])
                .status()
                .ok()?;
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

/// Update the Home page's CPU, RAM and temperature micro-meters from /proc and hwmon.
pub(crate) fn update_stats(ui: &Ui, cpu_prev: &mut Option<(u64, u64)>) {
    let set_meter = |i: usize, frac: f64, text: String, hot: bool| {
        ui.stat_bars[i].set_fraction(frac.clamp(0.0, 1.0));
        ui.stat_vals[i].set_label(&text);
        for w in [ui.stat_bars[i].upcast_ref::<gtk::Widget>(), ui.stat_vals[i].upcast_ref()] {
            if hot {
                w.add_css_class("hot");
            } else {
                w.remove_css_class("hot");
            }
        }
    };
    if let Some(line) = std::fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|s| s.lines().next().map(str::to_owned))
    {
        let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect();
        let total: u64 = v.iter().sum();
        let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
        let busy = total - idle;
        if let Some((pb, pt)) = cpu_prev.replace((busy, total))
            && total > pt
        {
            let pct = (busy - pb) * 100 / (total - pt);
            set_meter(0, pct as f64 / 100.0, format!("{pct}%"), pct >= 85);
        }
    }
    if let Ok(m) = std::fs::read_to_string("/proc/meminfo") {
        let kb = |key: &str| -> f64 {
            m.lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
                .unwrap_or(0.0)
        };
        let (total, avail) = (kb("MemTotal:"), kb("MemAvailable:"));
        if total > 0.0 {
            let used = total - avail;
            let frac = used / total;
            set_meter(1, frac, format!("{:.1}G", used / 1048576.0), frac >= 0.88);
        }
    }
    if let Some(t) = cpu_temp() {
        set_meter(2, ((t - 30) as f64 / 70.0).clamp(0.05, 1.0), format!("{t}°C"), t >= 75);
    }
}

pub(crate) fn cpu_temp() -> Option<i64> {
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
pub(crate) async fn clipboard_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let klipper = KlipperProxy::new(&conn).await?;
    let mut updates = klipper.receive_clipboard_history_updated().await?;
    loop {
        while let Some(c) = ui.clips.first_child() {
            ui.clips.remove(&c);
        }
        let history = klipper
            .get_clipboard_history_menu()
            .await
            .inspect_err(|e| eprintln!("clipboard: {e}"))
            .unwrap_or_default();
        for item in history.into_iter().take(3) {
            let flat: String = item.split_whitespace().collect::<Vec<_>>().join(" ");
            if flat.is_empty() {
                continue;
            }
            let short: String = if flat.chars().count() > 40 {
                flat.chars().take(40).collect::<String>() + "…"
            } else {
                flat
            };
            let btn = gtk::Button::builder().label(format!("📋  {short}")).css_classes(["clip"]).build();
            btn.child().and_then(|c| c.downcast::<gtk::Label>().ok()).inspect(|l| {
                l.set_xalign(0.0);
                l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                l.set_max_width_chars(28);
            });
            btn.set_hexpand(true);
            let qr = gtk::Button::builder()
                .icon_name("view-barcode-qr-symbolic")
                .tooltip_text(tr("Tampilkan QR", "Show QR code"))
                .css_classes(["panel-toggle"])
                .build();
            let (u, s, text) = (ui.clone(), state.clone(), item.clone());
            qr.connect_clicked(move |_| show_qr(&u, &s, &text));
            let k = klipper.clone();
            btn.connect_clicked(move |_| {
                let (k, item) = (k.clone(), item.clone());
                glib::spawn_future_local(async move {
                    let _ = k.set_clipboard_contents(&item).await;
                });
            });
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            row.append(&btn);
            row.append(&qr);
            ui.clips.append(&row);
        }
        refresh_compact(&ui, &state.borrow());
        if updates.next().await.is_none() {
            return Ok(());
        }
    }
}

/// Longest clipboard text offered as a QR code; beyond this the code gets too dense to scan
/// reliably off a laptop screen.
const QR_MAX_CHARS: usize = 500;

/// Show `text` as a QR code in place of the clipboard list.
fn show_qr(ui: &Ui, state: &Shared, text: &str) {
    if text.chars().count() > QR_MAX_CHARS {
        show_alert(ui, state, tr("Terlalu panjang untuk QR", "Too long for a QR code").into());
        return;
    }
    let Some((rgba, side)) = qr_rgba(text) else {
        return;
    };
    let tex = gtk::gdk::MemoryTexture::new(
        side as i32,
        side as i32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(rgba),
        side * 4,
    );
    ui.qr_pic.set_paintable(Some(&tex));
    ui.qr_view.set_visible(true);
    refresh_compact(ui, &state.borrow());
}

/// RGBA pixels of a QR code with a 2-module white border, 4 px per module (crisp, no scaling).
fn qr_rgba(text: &str) -> Option<(Vec<u8>, usize)> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let (modules, colors) = (code.width(), code.to_colors());
    const PX: usize = 4;
    const BORDER: usize = 2;
    let side = (modules + 2 * BORDER) * PX;
    let mut rgba = vec![255u8; side * side * 4];
    for (i, c) in colors.iter().enumerate() {
        if *c != qrcode::Color::Dark {
            continue;
        }
        let (mx, my) = (i % modules + BORDER, i / modules + BORDER);
        for y in my * PX..(my + 1) * PX {
            for x in mx * PX..(mx + 1) * PX {
                rgba[(y * side + x) * 4..(y * side + x) * 4 + 3].fill(0);
            }
        }
    }
    Some((rgba, side))
}

#[test]
fn qr_code_pixels() {
    let (rgba, side) = qr_rgba("https://github.com/Seteven2729/nusa-linux").unwrap();
    assert_eq!(rgba.len(), side * side * 4);
    assert_eq!(side % 4, 0);
    let px = |x: usize, y: usize| rgba[(y * side + x) * 4];
    assert_eq!(px(0, 0), 255, "white quiet border");
    assert_eq!(px(8, 8), 0, "top-left finder pattern is dark");
    assert!(qr_rgba(&"x".repeat(5000)).is_none(), "more than a QR code holds");
}

/// Phone notifications forwarded by KDE Connect (calls show up as the dialer's notification).
pub(crate) async fn phone_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.kde.kdeconnect.device.notifications")
        .member("notificationPosted")
        .build()?;
    let mut stream = MessageStream::for_match_rule(rule, &conn, None).await?;
    while let Some(Ok(msg)) = stream.next().await {
        let Ok(id) = msg.body().deserialize::<String>() else {
            continue;
        };
        let Some(path) = msg.header().path().map(|p| format!("{p}/{id}")) else {
            continue;
        };
        let Ok(n) = PhoneNotificationProxy::builder(&conn).path(path).build().await else {
            continue;
        };
        if n.silent().await.unwrap_or(false) {
            continue;
        }
        let (app, title, text) = (
            n.app_name().await.unwrap_or_default(),
            n.title().await.unwrap_or_default(),
            n.text().await.unwrap_or_default(),
        );
        let body = [title, text].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(": ");
        show_alert_for(&ui, &state, format!("📱  {app} · {body}"), 5);
    }
    Ok(())
}

/// Watches PipeWire through `pactl subscribe` and reports which apps record from a mic.
/// ponytail: shells out to pactl instead of linking libpulse/pipewire; swap if pactl ever goes missing.
/// PIDs of long-running helper processes (`pactl subscribe`, `pw-dump --monitor`), so Reload
/// can stop and reap them before exec. After exec the new program wouldn't know about them,
/// leaving zombies behind.
static WATCHERS: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn register_watcher(pid: i32) {
    WATCHERS.lock().unwrap().push(pid);
}

pub(crate) fn stop_watchers() {
    for pid in WATCHERS.lock().unwrap().drain(..) {
        // SAFETY: plain syscalls on child PIDs we spawned and never waited on elsewhere.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
            libc::waitpid(pid, std::ptr::null_mut(), 0);
        }
    }
}

pub(crate) fn spawn_mic_watch(ui: &Ui, state: &Shared) {
    let (tx, rx) = async_channel::unbounded::<Vec<String>>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let recording = || -> Vec<String> {
            let Ok(out) = std::process::Command::new("pactl")
                .args(["-f", "json", "list", "source-outputs"])
                .output()
            else {
                return vec![];
            };
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
        register_watcher(child.id() as i32);
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
                show_alert(&ui, &state, format!("{} · {name}", tr("🎙  Mic dipakai", "🎙  Mic in use")));
            } else {
                refresh_compact(&ui, &state.borrow());
            }
        }
    });
}

/// When idle, tint the notch with the wallpaper's main color (same recipe as album covers),
/// and follow Plasma's wallpaperChanged signal instead of polling.
pub(crate) async fn wallpaper_loop(ui: Ui) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let shell = PlasmaShellProxy::new(&conn).await?;
    let mut changes = shell.receive_wallpaper_changed().await?;
    loop {
        let url = shell
            .wallpaper(0)
            .await
            .ok()
            .and_then(|w| String::try_from(w.get("Image")?.try_clone().ok()?).ok())
            .unwrap_or_default();
        let accent = gio::spawn_blocking(move || {
            let path = gio::File::for_uri(&url).path()?;
            square_cover(&std::fs::read(wallpaper_image(&path)?).ok()?).map(|c| c.3)
        })
        .await
        .ok()
        .flatten();
        if let Some(rgb) = accent {
            ui.wall.load_from_string(&ambient_css("wall", rgb));
        }
        if changes.next().await.is_none() {
            return Ok(());
        }
    }
}

/// A wallpaper is either an image file or a KDE wallpaper package (a folder with
/// contents/images/<size>.<ext>); for a package take the largest image file.
pub(crate) fn wallpaper_image(path: &std::path::Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    std::fs::read_dir(path.join("contents/images"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .max_by_key(|p| p.metadata().map(|m| m.len()).unwrap_or(0))
}

#[test]
pub(crate) fn launcher_app_name() {
    assert_eq!(app_name("application://firefox.desktop"), "firefox");
    assert_eq!(app_name("application://org.mozilla.firefox.desktop"), "firefox");
    assert_eq!(app_name("application://google-chrome.desktop"), "google-chrome");
}
