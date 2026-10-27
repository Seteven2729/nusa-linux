//! Camera and screen-share indicators.
//!
//! Two sources, because apps reach the camera two ways: Chromium/Electron (Teams) open
//! /dev/video* directly, while Firefox, OBS and every Wayland screen share go through PipeWire.

use crate::*;

enum CaptureMsg {
    /// Apps holding a /dev/video* device open.
    V4l(Vec<String>),
    /// From PipeWire: (a camera node is streaming, a screen-share node exists).
    Pw(bool, bool),
}

pub(crate) fn spawn_capture_watch(ui: &Ui, state: &Shared) {
    let (tx, rx) = async_channel::unbounded::<CaptureMsg>();
    let t = tx.clone();
    std::thread::spawn(move || watch_video_devices(t));
    std::thread::spawn(move || watch_pipewire(tx));
    let (ui, state) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        while let Ok(msg) = rx.recv().await {
            let (cam_before, share_before) = {
                let st = state.borrow();
                (!st.cam_apps.is_empty() || st.cam_pw, st.sharing)
            };
            match msg {
                CaptureMsg::V4l(apps) => state.borrow_mut().cam_apps = apps,
                CaptureMsg::Pw(cam, share) => {
                    let mut st = state.borrow_mut();
                    st.cam_pw = cam;
                    st.sharing = share;
                }
            }
            let (cam_now, share_now, apps) = {
                let st = state.borrow();
                (!st.cam_apps.is_empty() || st.cam_pw, st.sharing, st.cam_apps.join(", "))
            };
            if share_now && !share_before {
                show_alert(&ui, &state, tr("🖥  Layar sedang di-share", "🖥  Screen is being shared").into());
            } else if cam_now && !cam_before {
                let who = if apps.is_empty() { String::new() } else { format!(" · {apps}") };
                show_alert(&ui, &state, format!("{}{who}", tr("📷  Kamera dipakai", "📷  Camera in use")));
            } else {
                refresh_compact(&ui, &state.borrow());
            }
        }
    });
}

/// inotify on each /dev/video* tells us when anything opens or closes a camera; then we look
/// up who holds it. No polling.
/// ponytail: watches the devices present at startup; a camera plugged in later needs a Reload.
fn watch_video_devices(tx: async_channel::Sender<CaptureMsg>) {
    let devices: Vec<PathBuf> = std::fs::read_dir("/dev")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("video")))
        .collect();
    if devices.is_empty() {
        return;
    }
    // SAFETY: plain inotify syscalls on a descriptor owned by this thread.
    let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
    if fd < 0 {
        return;
    }
    for dev in &devices {
        let Ok(path) = std::ffi::CString::new(dev.as_os_str().as_encoded_bytes()) else { continue };
        unsafe { libc::inotify_add_watch(fd, path.as_ptr(), libc::IN_OPEN | libc::IN_CLOSE) };
    }
    let mut last = camera_holders();
    let _ = tx.send_blocking(CaptureMsg::V4l(last.clone()));
    let mut buf = [0u8; 4096];
    loop {
        if unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } <= 0 {
            return;
        }
        // PipeWire probes cameras with quick open/close bursts; let them settle before looking.
        std::thread::sleep(Duration::from_millis(300));
        let now = camera_holders();
        if now != last {
            last = now;
            if tx.send_blocking(CaptureMsg::V4l(last.clone())).is_err() {
                return;
            }
        }
    }
}

/// Names of processes with a /dev/video* open, leaving out PipeWire itself (its camera use
/// shows up as a running node in watch_pipewire, with the real app behind it).
fn camera_holders() -> Vec<String> {
    let mut apps: Vec<String> = std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|p| p.file_name().to_str().is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit())))
        .filter(|p| {
            std::fs::read_dir(p.path().join("fd")).into_iter().flatten().flatten().any(|fd| {
                std::fs::read_link(fd.path()).is_ok_and(|t| t.to_string_lossy().starts_with("/dev/video"))
            })
        })
        .filter_map(|p| std::fs::read_to_string(p.path().join("comm")).ok())
        .map(|c| c.trim().to_owned())
        .filter(|c| !["pipewire", "wireplumber"].contains(&c.as_str()))
        .collect();
    apps.sort();
    apps.dedup();
    apps
}

/// `pw-dump --monitor` prints the PipeWire graph, then every change, as a stream of JSON arrays.
fn watch_pipewire(tx: async_channel::Sender<CaptureMsg>) {
    let mut cmd = std::process::Command::new("pw-dump");
    cmd.args(["--monitor", "--no-colors"]).stdout(std::process::Stdio::piped());
    // SAFETY: prctl is async-signal-safe, which is all pre_exec requires.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(&mut cmd, || {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let Ok(mut child) = cmd.spawn() else { return };
    register_watcher(child.id() as i32);
    let mut nodes = HashMap::new();
    let mut last = (false, false);
    for update in serde_json::Deserializer::from_reader(child.stdout.take().unwrap()).into_iter::<serde_json::Value>() {
        let Ok(update) = update else { return };
        let now = apply_pw_update(&mut nodes, &update);
        if now != last {
            last = now;
            if tx.send_blocking(CaptureMsg::Pw(now.0, now.1)).is_err() {
                return;
            }
        }
    }
}

/// Video sources PipeWire knows about: id -> (is a camera, state).
type VideoNodes = HashMap<u64, (bool, String)>;

/// Fold one pw-dump batch into `nodes`; returns (camera streaming, screen being shared).
/// A camera node always exists and only counts while "running". Any other video source
/// (KWin's screencast stream, for one) exists only while something is shared.
fn apply_pw_update(nodes: &mut VideoNodes, update: &serde_json::Value) -> (bool, bool) {
    for obj in update.as_array().into_iter().flatten() {
        let Some(id) = obj["id"].as_u64() else { continue };
        let info = &obj["info"];
        if info.is_null() {
            nodes.remove(&id);
            continue;
        }
        let props = &info["props"];
        if props["media.class"].as_str() == Some("Video/Source") {
            let camera = matches!(props["device.api"].as_str(), Some("v4l2" | "libcamera"));
            nodes.insert(id, (camera, info["state"].as_str().unwrap_or("").to_owned()));
        } else if let Some((_, state)) = nodes.get_mut(&id) {
            // Later updates may carry only the changed state.
            if let Some(s) = info["state"].as_str() {
                *state = s.to_owned();
            }
        }
    }
    let camera = nodes.values().any(|(cam, state)| *cam && state == "running");
    let sharing = nodes.values().any(|(cam, _)| !cam);
    (camera, sharing)
}

#[test]
fn pipewire_capture_state() {
    let mut nodes = VideoNodes::new();
    let cam = |state: &str| {
        serde_json::json!([{ "id": 84, "info": { "state": state, "props": { "media.class": "Video/Source", "device.api": "v4l2" } } }])
    };
    assert_eq!(apply_pw_update(&mut nodes, &cam("suspended")), (false, false), "idle camera");
    assert_eq!(apply_pw_update(&mut nodes, &cam("running")), (true, false), "camera in use");
    let share = serde_json::json!([{ "id": 200, "info": { "state": "running", "props": { "media.class": "Video/Source", "node.name": "kwin" } } }]);
    assert_eq!(apply_pw_update(&mut nodes, &share), (true, true), "screen share starts");
    let gone = serde_json::json!([{ "id": 200, "info": null }, { "id": 84, "info": { "state": "suspended" } }]);
    assert_eq!(apply_pw_update(&mut nodes, &gone), (false, false), "share ends, camera stops");
    let audio = serde_json::json!([{ "id": 5, "info": { "props": { "media.class": "Audio/Source" } } }]);
    assert_eq!(apply_pw_update(&mut nodes, &audio), (false, false), "non-video ignored");
}
