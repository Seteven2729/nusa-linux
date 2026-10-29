//! Volume mixer page: the output volume, then a slider and mute button per app playing audio.

use crate::*;

/// One app's playback stream (a PipeWire/Pulse "sink input").
struct AppStream {
    index: u32,
    name: String,
    /// 0.0..=1.0 (louder than 100% is clamped on the slider).
    volume: f64,
    muted: bool,
}

fn list_app_streams() -> Vec<AppStream> {
    let Ok(out) = std::process::Command::new("pactl")
        .args(["-f", "json", "list", "sink-inputs"])
        .output()
    else {
        return vec![];
    };
    let list: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let props = &s["properties"];
            let name = app_label(
                props["application.name"]
                    .as_str()
                    .or(props["application.process.binary"].as_str())?,
            );
            // Channels are usually equal; take the loudest.
            let volume = s["volume"]
                .as_object()?
                .values()
                .filter_map(|c| {
                    c["value_percent"]
                        .as_str()?
                        .trim_end_matches('%')
                        .parse::<f64>()
                        .ok()
                })
                .fold(0.0, f64::max)
                / 100.0;
            Some(AppStream {
                index: s["index"].as_u64()? as u32,
                name,
                volume,
                muted: s["mute"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// "PipeWire ALSA [strawberry]" -> "Strawberry"; "Firefox" stays.
fn app_label(raw: &str) -> String {
    let inner = raw
        .rsplit_once('[')
        .and_then(|(_, r)| r.strip_suffix(']'))
        .unwrap_or(raw)
        .trim();
    let mut c = inner.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn output_volume() -> (f64, bool) {
    let out = std::process::Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output();
    // "Volume: 0.45" or "Volume: 0.45 [MUTED]"
    let text = out
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let vol = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    (vol, text.contains("[MUTED]"))
}

/// Run a volume command off the main thread.
fn run(cmd: &'static str, args: Vec<String>) {
    gio::spawn_blocking(move || {
        let _ = std::process::Command::new(cmd).args(&args).status();
    });
}

/// A 0–100 slider that applies at most every 80 ms while dragged, instead of spawning
/// pactl/wpctl for every pixel moved.
fn volume_slider(value: f64, apply: impl Fn(u32) + 'static) -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_value((value * 100.0).clamp(0.0, 100.0));
    scale.set_hexpand(true);
    let pending: Rc<std::cell::Cell<Option<u32>>> = Default::default();
    let apply = Rc::new(apply);
    scale.connect_value_changed(move |s| {
        let first = pending.replace(Some(s.value().round() as u32)).is_none();
        if first {
            let (pending, apply) = (pending.clone(), apply.clone());
            glib::timeout_add_local_once(Duration::from_millis(80), move || {
                if let Some(v) = pending.take() {
                    apply(v);
                }
            });
        }
    });
    scale
}

/// Rebuild the page from the current streams. Reads happen off the main thread.
pub(crate) fn fill_mixer_page(ui: &Ui) {
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let Ok(((master, master_muted), streams)) =
            gio::spawn_blocking(|| (output_volume(), list_app_streams())).await
        else {
            return;
        };
        while let Some(c) = ui.mixer_box.first_child() {
            ui.mixer_box.remove(&c);
        }
        let row = |name: &str, slider: &gtk::Scale, muted: bool, on_mute: Box<dyn Fn(bool)>| {
            let r = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let label = gtk::Label::builder()
                .label(name)
                .css_classes(["row-label"])
                .xalign(0.0)
                .width_request(92)
                .max_width_chars(12)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let mute = gtk::ToggleButton::builder()
                .icon_name(if muted {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                })
                .active(muted)
                .css_classes(["panel-toggle"])
                .tooltip_text(tr("Bisukan", "Mute"))
                .build();
            mute.connect_toggled(move |b| {
                b.set_icon_name(if b.is_active() {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                });
                on_mute(b.is_active());
            });
            r.append(&label);
            r.append(slider);
            r.append(&mute);
            r
        };
        let out = volume_slider(master, |v| {
            run(
                "wpctl",
                vec![
                    "set-volume".into(),
                    "-l".into(),
                    "1.0".into(),
                    "@DEFAULT_AUDIO_SINK@".into(),
                    format!("{v}%"),
                ],
            )
        });
        ui.mixer_box.append(&row(
            "Output",
            &out,
            master_muted,
            Box::new(|m| {
                run(
                    "wpctl",
                    vec![
                        "set-mute".into(),
                        "@DEFAULT_AUDIO_SINK@".into(),
                        if m { "1" } else { "0" }.into(),
                    ],
                )
            }),
        ));
        if streams.is_empty() {
            let none = tr(
                "Tidak ada aplikasi yang memutar suara",
                "No apps playing audio",
            );
            ui.mixer_box.append(
                &gtk::Label::builder()
                    .label(none)
                    .css_classes(["time"])
                    .xalign(0.0)
                    .build(),
            );
        }
        for s in streams {
            let idx = s.index.to_string();
            let i = idx.clone();
            let slider = volume_slider(s.volume, move |v| {
                run(
                    "pactl",
                    vec!["set-sink-input-volume".into(), i.clone(), format!("{v}%")],
                )
            });
            ui.mixer_box.append(&row(
                &s.name,
                &slider,
                s.muted,
                Box::new(move |m| {
                    run(
                        "pactl",
                        vec![
                            "set-sink-input-mute".into(),
                            idx.clone(),
                            if m { "1" } else { "0" }.into(),
                        ],
                    )
                }),
            ));
        }
    });
}

/// Refill the page when an app starts or stops playing audio, if the page is showing.
/// Volume changes made elsewhere aren't mirrored, so a slider is never yanked mid-drag.
pub(crate) fn spawn_mixer_watch(ui: &Ui) {
    let (tx, rx) = async_channel::unbounded::<()>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut cmd = std::process::Command::new("pactl");
        cmd.arg("subscribe").stdout(std::process::Stdio::piped());
        // SAFETY: prctl is async-signal-safe, which is all pre_exec requires.
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut cmd, || {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let Ok(mut child) = cmd.spawn() else { return };
        register_watcher(child.id() as i32);
        for line in std::io::BufReader::new(child.stdout.take().unwrap())
            .lines()
            .map_while(Result::ok)
        {
            // "Event 'new' on sink-input #343" / "Event 'remove' on sink-input #343"
            if line.contains("sink-input")
                && !line.contains("'change'")
                && tx.send_blocking(()).is_err()
            {
                return;
            }
        }
    });
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        while rx.recv().await.is_ok() {
            if ui.pages.visible_child_name().as_deref() == Some("mixer") {
                fill_mixer_page(&ui);
            }
        }
    });
}

#[test]
fn mixer_app_labels() {
    assert_eq!(app_label("PipeWire ALSA [strawberry]"), "Strawberry");
    assert_eq!(app_label("Firefox"), "Firefox");
    assert_eq!(app_label("teams-for-linux"), "Teams-for-linux");
}
