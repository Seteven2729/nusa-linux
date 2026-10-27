//! System events shown in the notch: power, network, Bluetooth, downloads, mic, clipboard, phone, stats, wallpaper.

use crate::*;

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
    let mut is_player: HashMap<String, bool> = HashMap::new();
    while let Some(Ok(msg)) = stream.next().await {
        let Some(sender) = msg.header().sender().map(|s| s.to_string()) else { continue };
        if !is_player.contains_key(&sender) {
            let mut player = false;
            for name in dbus.list_names().await.unwrap_or_default() {
                if name.starts_with("org.mpris.MediaPlayer2.")
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
        let Ok((uri, props)) = msg.body().deserialize::<(String, HashMap<String, OwnedVariant>)>() else { continue };
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
        let mut active: Vec<(String, f64)> =
            entries.iter().filter(|(_, (_, v))| *v).map(|(u, (p, _))| (app_name(u), *p)).collect();
        active.sort_by(|a, b| a.0.cmp(&b.0));
        state.borrow_mut().downloads = active;
        if visible {
            // Most recently updated download wins when several run at once.
            state.borrow_mut().download = Some((app, progress));
            refresh_compact(&ui, &state.borrow());
        } else if was_visible {
            let showing = state.borrow().download.as_ref().is_some_and(|(a, _)| *a == app);
            if showing {
                state.borrow_mut().download = None;
            }
            show_alert(&ui, &state, format!("{} · {app}", tr("⬇  Download selesai", "⬇  Download finished")));
        }
        if ui.pages.visible_child_name().as_deref() == Some("downloads") {
            fill_downloads_page(&ui, &state.borrow());
        }
    }
    Ok(())
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
    for (app, p) in &st.downloads {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(&gtk::Label::builder().label(app.as_str()).hexpand(true).xalign(0.0).build());
        row.append(&muted(&format!("{}%", (p * 100.0).round())));
        ui.dl_active.append(&row);
        ui.dl_active.append(&gtk::ProgressBar::builder().fraction(*p).css_classes(["dl"]).build());
    }
    let files = recent_downloads(&downloads_dir(), 4);
    if files.is_empty() {
        ui.dl_recent.append(&muted(tr("Folder Download kosong", "Downloads folder is empty")));
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
        row.append(&muted(&format!("{} · {}", human_size(size, cfg().english), ago(age, cfg().english))));
        let btn = gtk::Button::builder().child(&row).css_classes(["clip"]).build();
        btn.connect_clicked(move |_| {
            let _ = gio::AppInfo::launch_default_for_uri(&gio::File::for_path(&path).uri(), None::<&gio::AppLaunchContext>);
        });
        ui.dl_recent.append(&btn);
    }
}

/// Newest finished files in `dir`: (path, size, modified). Skips folders, hidden files and
/// browsers' in-progress files.
fn recent_downloads(dir: &std::path::Path, n: usize) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let partial = [".part", ".crdownload", ".download", ".tmp"];
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !name.starts_with('.') && !partial.iter().any(|p| name.ends_with(p))
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
    let s = if u == 0 { format!("{bytes} B") } else { format!("{:.1} {}", v, units[u]) };
    if english { s } else { s.replace('.', ",") }
}

/// "baru saja", "5 mnt", "3 jam", "2 hari" (or "just now", "5 min", "3 h", "2 d").
fn ago(secs: u64, english: bool) -> String {
    let (now, min, hour, day) = if english { ("just now", "min", "h", "d") } else { ("baru saja", "mnt", "jam", "hari") };
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
    for (name, delay) in [("old.pdf", 0), ("movie.mkv.part", 20), ("new.zip", 20), (".hidden", 20)] {
        std::thread::sleep(Duration::from_millis(delay));
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    let names: Vec<_> = recent_downloads(&dir, 4).iter().map(|f| f.0.file_name().unwrap().to_owned()).collect();
    assert_eq!(names, ["new.zip", "old.pdf"], "newest first, partial and hidden files skipped");
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
            let icon = if active.kind().await.as_deref() == Ok("802-11-wireless") { "📶" } else { "🔌" };
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
            show_alert(&ui, &state, format!("🎧  {name} {}", tr("terputus", "disconnected")));
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

pub(crate) fn change_volume(ui: &Ui, state: &Shared, step: &'static str) {
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
pub(crate) fn read_stats(cpu_prev: &mut Option<(u64, u64)>) -> String {
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
        let history = klipper.get_clipboard_history_menu().await.inspect_err(|e| eprintln!("clipboard: {e}")).unwrap_or_default();
        for item in history.into_iter().take(3) {
            let flat: String = item.split_whitespace().collect::<Vec<_>>().join(" ");
            if flat.is_empty() {
                continue;
            }
            let short: String = if flat.chars().count() > 40 { flat.chars().take(40).collect::<String>() + "…" } else { flat };
            let btn = gtk::Button::builder().label(format!("📋  {short}")).css_classes(["clip"]).build();
            btn.child().and_then(|c| c.downcast::<gtk::Label>().ok()).inspect(|l| {
                l.set_xalign(0.0);
                l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                l.set_max_width_chars(28);
            });
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
pub(crate) async fn phone_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
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
        if let Some((r, g, b)) = accent {
            ui.wall.load_from_string(&format!(
                ".notch.wall {{ background: linear-gradient(160deg, {} 0%, rgb({},{},{}) 65%, rgb({r},{g},{b}) 100%); }}",
                cfg().gradient[0],
                r / 3,
                g / 3,
                b / 3
            ));
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
