//! Tray menu, and starting Nusa at login through a systemd user service.

use crate::*;

const SERVICE: &str = "nusa-linux.service";

fn systemd_dir() -> PathBuf {
    glib::user_config_dir().join("systemd/user")
}

/// The XDG autostart entry older versions wrote; replaced by the systemd service.
fn old_autostart_file() -> PathBuf {
    glib::user_config_dir().join("autostart/nusa-linux.desktop")
}

fn autostart_enabled() -> bool {
    systemd_dir().join("graphical-session.target.wants").join(SERVICE).exists()
}

/// Start with the graphical session, and come back on its own after a crash.
/// Exit from the tray quits with status 0, which Restart=on-failure leaves alone.
fn set_autostart(on: bool) {
    let systemctl = |args: &[&str]| {
        let _ = std::process::Command::new("systemctl").arg("--user").args(args).status();
    };
    if on {
        let exe = std::env::current_exe().unwrap_or_default();
        let unit = format!(
            "[Unit]\nDescription=Nusa notch\nPartOf=graphical-session.target\nAfter=graphical-session.target\n\n\
             [Service]\nExecStart={}\nRestart=on-failure\nRestartSec=2\n\n\
             [Install]\nWantedBy=graphical-session.target\n",
            exe.display()
        );
        let dir = systemd_dir();
        if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join(SERVICE), unit)).is_ok() {
            systemctl(&["daemon-reload"]);
            systemctl(&["enable", SERVICE]);
        }
    } else {
        systemctl(&["disable", SERVICE]);
    }
    // Two autostart mechanisms would race to start Nusa; keep only the service.
    let _ = std::fs::remove_file(old_autostart_file());
}

/// One-time move from the old XDG autostart entry to the service.
pub(crate) fn migrate_autostart() {
    if old_autostart_file().exists() {
        std::thread::spawn(|| set_autostart(true));
    }
}

pub(crate) enum TrayMsg {
    Minimal(bool),
    Mascot(bool),
    Reload,
    Quit,
}

pub(crate) struct Tray {
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
                checked: autostart_enabled(),
                activate: Box::new(|_| set_autostart(!autostart_enabled())),
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
                label: "Edit settings".into(),
                activate: Box::new(|_| {
                    let _ = std::process::Command::new("xdg-open").arg(config_path()).spawn();
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Reload".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send_blocking(TrayMsg::Reload);
                }),
                ..Default::default()
            }
            .into(),
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

pub(crate) fn spawn_tray(app: &gtk::Application, ui: &Ui, state: &Shared) {
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
                    save_setting("minimal", on);
                    state.borrow_mut().minimal = on;
                    refresh_compact(&ui, &state.borrow());
                }
                TrayMsg::Mascot(on) => {
                    save_setting("mascot", on);
                    state.borrow_mut().mascot_off = !on;
                    if !on {
                        set_mood(&ui, &state, Mood::None);
                    }
                }
                // Settings are read once at startup, so reloading means starting over in place.
                // exec keeps the PID, so a systemd service doesn't see it as an exit.
                TrayMsg::Reload => {
                    use std::os::unix::process::CommandExt;
                    stop_watchers();
                    let err = std::process::Command::new(std::env::current_exe().unwrap_or_default()).exec();
                    show_alert(&ui, &state, format!("Reload gagal: {err}"));
                }
                TrayMsg::Quit => app.quit(),
            }
        }
    });
}
