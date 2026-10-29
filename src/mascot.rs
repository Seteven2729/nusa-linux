//! The idle mascot: drawing, compositor idle notifications, mood changes.

use crate::*;

/// What the music is doing, for the mascot's expression.
#[derive(Clone, Copy)]
pub(crate) enum Music {
    None,
    Playing(bool),
    Paused,
}

pub(crate) fn draw_eyes(
    cr: &gtk::cairo::Context,
    w: f64,
    h: f64,
    mood: Mood,
    blink: bool,
    music: Music,
) {
    use std::f64::consts::PI;
    let (ew, eh, gap) = (9.0, 13.0, 8.0);
    // Music overrides the idle expression (except the hello wave): happy eyes bobbing to the
    // beat while playing, drowsy flat eyes while paused.
    let mood = match (mood, music) {
        (Mood::Wave, _) | (_, Music::None) => mood,
        (_, Music::Playing(_)) => Mood::Wave,
        (_, Music::Paused) => Mood::Sleep,
    };
    let cy = h / 2.0
        + if let Music::Playing(up) = music {
            if up { -2.5 } else { 2.5 }
        } else {
            0.0
        };
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
            Mood::Sleep if matches!(music, Music::Paused) => {
                // Drowsy: flat, half-shut lids.
                cr.move_to(x, cy + 1.0);
                cr.line_to(x + ew, cy + 1.0);
                let _ = cr.stroke();
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
    if mood == Mood::Sleep && !matches!(music, Music::Paused) {
        cr.set_font_size(9.0);
        cr.move_to(w / 2.0 + gap / 2.0 + ew + 3.0, cy - 2.0);
        let _ = cr.show_text("z");
    }
}

/// Idle levels from the compositor: 1 = quiet for `timing.mascot_awake`, 2 = for `timing.mascot_sleep`.
pub(crate) struct IdleWatch(async_channel::Sender<(u8, bool)>);

impl
    wayland_client::Dispatch<
        wayland_client::protocol::wl_registry::WlRegistry,
        wayland_client::globals::GlobalListContents,
    > for IdleWatch
{
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
pub(crate) fn spawn_idle_watch(ui: &Ui, state: &Shared) {
    let (tx, rx) = async_channel::unbounded();
    std::thread::spawn(move || {
        let run = || -> Result<(), Box<dyn std::error::Error>> {
            use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;
            let conn = wayland_client::Connection::connect_to_env()?;
            let (globals, mut queue) =
                wayland_client::globals::registry_queue_init::<IdleWatch>(&conn)?;
            let qh = queue.handle();
            let seat: wayland_client::protocol::wl_seat::WlSeat = globals.bind(&qh, 1..=1, ())?;
            let notifier: ExtIdleNotifierV1 = globals.bind(&qh, 1..=1, ())?;
            notifier.get_idle_notification(cfg().mascot_awake * 1000, &seat, &qh, 1u8);
            notifier.get_idle_notification(cfg().mascot_sleep * 1000, &seat, &qh, 2u8);
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

pub(crate) fn set_mood(ui: &Ui, state: &Shared, mood: Mood) {
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
