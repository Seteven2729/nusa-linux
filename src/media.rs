//! Music: picking the MPRIS player, reading the track, cover art and its accent color.

use crate::*;

pub(crate) async fn mpris_loop(ui: Ui, state: Shared) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.DBus.Properties")
        .member("PropertiesChanged")
        .path("/org/mpris/MediaPlayer2")
        .build()?;
    let props = MessageStream::for_match_rule(rule, &conn, None)
        .await?
        .map(|_| ());
    let owners = dbus
        .receive_name_owner_changed()
        .await?
        .filter(|s| {
            std::future::ready(
                s.args()
                    .is_ok_and(|a| a.name().starts_with("org.mpris.MediaPlayer2.")),
            )
        })
        .map(|_| ());
    // The player switch button asks for a re-pick through this channel.
    let (repick_tx, repick_rx) = async_channel::unbounded::<()>();
    state.borrow_mut().repick = Some(repick_tx);
    // async_channel's Receiver isn't Unpin, so pin the merged stream to poll it.
    let mut events = std::pin::pin!(futures_util::stream::select(
        futures_util::stream::select(props, owners),
        repick_rx
    ));

    // Re-pick from every live player on each event instead of tracking incrementally,
    // so closed players and failed calls can never leave a stale choice behind.
    loop {
        let current = state
            .borrow()
            .player
            .as_ref()
            .map(|p| p.inner().destination().to_string());
        let pinned = state.borrow().pinned.clone();
        let (picked, players) =
            pick_player(&conn, &dbus, current.as_deref(), pinned.as_deref()).await;
        {
            let mut st = state.borrow_mut();
            // A pinned player that closed releases the pin; picking goes back to automatic.
            if st
                .pinned
                .as_ref()
                .is_some_and(|p| !players.iter().any(|(o, _)| o == p))
            {
                st.pinned = None;
            }
            st.players = players;
        }
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
/// Every live player as (bus owner, display name), plus the one to show: the pinned one if
/// alive, else current if still playing, else any playing one, else current, else any.
pub(crate) async fn pick_player(
    conn: &Connection,
    dbus: &zbus::fdo::DBusProxy<'_>,
    current: Option<&str>,
    pinned: Option<&str>,
) -> (Option<PlayerProxy<'static>>, Vec<(String, String)>) {
    let mut alive = vec![];
    for name in dbus.list_names().await.unwrap_or_default() {
        let Some(suffix) = name.strip_prefix("org.mpris.MediaPlayer2.") else {
            continue;
        };
        // Debug builds: NUSA_ONLY_PLAYER=<text> ignores players whose bus name lacks it, so
        // tools/ can test with a fake player while the user's real one keeps playing.
        #[cfg(debug_assertions)]
        if std::env::var("NUSA_ONLY_PLAYER").is_ok_and(|o| !name.contains(&o)) {
            continue;
        }
        let Ok(owner) = dbus.get_name_owner(name.as_ref()).await else {
            continue;
        };
        let Ok(p) = player(conn, owner.to_string()).await else {
            continue;
        };
        let Ok(status) = p.playback_status().await else {
            continue;
        };
        let identity = match MediaPlayerProxy::builder(conn)
            .destination(owner.to_string())
            .build()
            .await
        {
            Ok(mp) => mp.identity().await.ok(),
            Err(_) => None,
        };
        // "firefox.instance_1_23" -> "firefox" when the player has no Identity.
        let label =
            identity.unwrap_or_else(|| suffix.split('.').next().unwrap_or(suffix).to_owned());
        alive.push((owner.to_string(), label, status == "Playing", p));
    }
    let is = |o: &str, want: Option<&str>| Some(o) == want;
    let idx = alive
        .iter()
        .position(|(o, ..)| is(o, pinned))
        .or_else(|| {
            alive
                .iter()
                .position(|(o, _, playing, _)| *playing && is(o, current))
        })
        .or_else(|| alive.iter().position(|(_, _, playing, _)| *playing))
        .or_else(|| alive.iter().position(|(o, ..)| is(o, current)))
        .or((!alive.is_empty()).then_some(0));
    let players = alive
        .iter()
        .map(|(o, l, ..)| (o.clone(), l.clone()))
        .collect();
    (idx.map(|i| alive.swap_remove(i).3), players)
}

/// Forget the current player and its track, leaving everything else (alerts, mic, settings, mascot) alone.
pub(crate) fn clear_player(ui: &Ui, state: &Shared) {
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

pub(crate) async fn player(conn: &Connection, dest: String) -> zbus::Result<PlayerProxy<'static>> {
    PlayerProxy::builder(conn)
        .destination(dest)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
}

pub(crate) async fn refresh_player(ui: &Ui, state: &Shared) {
    let Some(p) = state.borrow().player.clone() else {
        return;
    };
    let (Ok(meta), Ok(status)) = (p.metadata().await, p.playback_status().await) else {
        // Player went away.
        clear_player(ui, state);
        return;
    };
    let str_of = |k: &str| {
        meta.get(k)
            .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
            .unwrap_or_default()
    };
    let title = str_of("xesam:title");
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok())
        .map(|a| a.join(", "))
        .unwrap_or_default();
    let length_s = meta
        .get("mpris:length")
        .and_then(|v| {
            i64::try_from(v.try_clone().ok()?)
                .ok()
                .or_else(|| u64::try_from(v.try_clone().ok()?).ok().map(|n| n as i64))
        })
        .unwrap_or(0)
        / 1_000_000;

    let playing = status == "Playing";
    let art_url = str_of("mpris:artUrl");
    if state.borrow().art_url != art_url {
        state.borrow_mut().art_url = art_url.clone();
        let (ui, state) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            let url = art_url.clone();
            let cover = gio::spawn_blocking(move || square_cover(&load_art(&url)?))
                .await
                .ok()
                .flatten();
            if let Some((_, _, _, rgb)) = &cover {
                ui.accent.load_from_string(&ambient_css("tinted", *rgb));
            }
            if state.borrow().art_url != art_url {
                return; // track changed while loading
            }
            let tex = cover.map(|(pixels, alpha, stride, _)| {
                let format = if alpha {
                    gtk::gdk::MemoryFormat::R8g8b8a8
                } else {
                    gtk::gdk::MemoryFormat::R8g8b8
                };
                gtk::gdk::MemoryTexture::new(112, 112, format, &pixels, stride)
                    .upcast::<gtk::gdk::Texture>()
            });
            ui.art.set_paintable(tex.as_ref());
            ui.mini_art.set_paintable(tex.as_ref());
            ui.art.set_visible(tex.is_some());
            refresh_compact(&ui, &state.borrow());
        });
    }
    state.borrow_mut().track_id = meta
        .get("mpris:trackid")
        .and_then(|v| OwnedObjectPath::try_from(v.try_clone().ok()?).ok());
    let len = length_s.max(1) as f64;
    let pos_us = p.position().await.unwrap_or(0).max(0);
    let pos_s = (pos_us as f64 / 1e6).clamp(0.0, len);
    ui.seek.set_range(0.0, len);
    if state.borrow().seeking.is_none() {
        ui.seek.set_value(pos_s);
        ui.time.set_label(&format!(
            "{} / {}",
            mmss(pos_s as i64),
            mmss(length_s.max(1))
        ));
    }
    ui.song_bar.set_fraction((pos_s / len).clamp(0.0, 1.0));
    ui.seek.parent().unwrap().set_visible(length_s > 0);
    ui.title.set_label(&title);
    ui.artist.set_label(&artist);
    ui.play.set_icon_name(if playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    });

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
            // Fetch and romanize off the main thread: lrclib can take seconds, and kakasi
            // loads its dictionary on first use.
            let (lyrics, rom) = gio::spawn_blocking(move || {
                let lyrics = if cfg().lyrics {
                    fetch_lyrics(&t, &artist, length_s)
                } else {
                    vec![]
                };
                let rom = if cfg().romanize {
                    romanize_song(&lyrics)
                } else {
                    vec![None; lyrics.len()]
                };
                (lyrics, rom)
            })
            .await
            .unwrap_or_default();
            if state.borrow().track == title {
                for ((_, line), r) in lyrics.iter().zip(&rom) {
                    let item = gtk::Box::new(gtk::Orientation::Vertical, 1);
                    item.add_css_class("lrc");
                    let text = if line.is_empty() {
                        "♪"
                    } else {
                        line.as_str()
                    };
                    let main = gtk::Label::builder()
                        .label(text)
                        .css_classes(["lrc-main"])
                        .xalign(0.0)
                        .wrap(true)
                        .max_width_chars(28)
                        .build();
                    item.append(&main);
                    if let Some(r) = r {
                        let sub = gtk::Label::builder()
                            .label(r.as_str())
                            .css_classes(["lrc-rom"])
                            .xalign(0.0)
                            .wrap(true)
                            .max_width_chars(32)
                            .build();
                        item.append(&sub);
                    }
                    ui.lyrics_list.append(&item);
                }
                state.borrow_mut().lyrics = lyrics;
                state.borrow_mut().lyrics_rom = rom;
                let idx = current_index(&state.borrow().lyrics, pos_us / 1000);
                let line = current_line(&state.borrow().lyrics, pos_us / 1000).unwrap_or_default();
                let r = idx.and_then(|i| state.borrow().lyrics_rom.get(i).cloned().flatten());
                ui.lyric.set_label(&line);
                ui.lyric_rom.set_label(r.as_deref().unwrap_or_default());
                highlight_lyric(&ui, &mut state.borrow_mut(), idx);
                refresh_compact(&ui, &state.borrow());
            }
        });
    }
}

/// Stage-style lyrics focus: active line `.now` in full size, ±1 lines `.near`, ±2 lines `.mid`.
pub(crate) fn highlight_lyric(ui: &Ui, st: &mut State, idx: Option<usize>) {
    if st.lyric_idx == idx {
        return;
    }
    let len = st.lyrics.len();
    for i in 0..len {
        let Some(child) = ui
            .lyrics_list
            .row_at_index(i as i32)
            .and_then(|r| r.child())
        else {
            continue;
        };
        for c in ["now", "near", "mid"] {
            child.remove_css_class(c);
        }
        if let Some(active) = idx {
            match i.abs_diff(active) {
                0 => child.add_css_class("now"),
                1 => child.add_css_class("near"),
                2 => child.add_css_class("mid"),
                _ => {}
            }
        }
    }
    center_lyric(ui, idx, ui.lyrics_rev.is_child_revealed());
    st.lyric_idx = idx;
}

/// Center `idx` inside `lyrics_scroll`, easing over 220 ms when `smooth` is true.
pub(crate) fn center_lyric(ui: &Ui, idx: Option<usize>, smooth: bool) {
    let Some(new) = idx
        .and_then(|i| ui.lyrics_list.row_at_index(i as i32))
        .and_then(|r| r.child())
    else {
        return;
    };
    let Some(b) = new.compute_bounds(&ui.lyrics_list) else {
        return;
    };
    let seq = ui.lyric_anim.get().wrapping_add(1);
    ui.lyric_anim.set(seq);
    let adj = ui.lyrics_scroll.vadjustment();
    let target = (b.y() as f64 + b.height() as f64 / 2.0 - adj.page_size() / 2.0).max(0.0);
    if !smooth {
        adj.set_value(target);
        return;
    }
    let start = adj.value();
    if (target - start).abs() < 1.0 {
        return;
    }
    let t0 = std::time::Instant::now();
    let anim = ui.lyric_anim.clone();
    ui.lyrics_scroll.add_tick_callback(move |_, _| {
        if anim.get() != seq {
            return glib::ControlFlow::Break;
        }
        let p = (t0.elapsed().as_secs_f64() / 0.22).min(1.0);
        let ease = 1.0 - (1.0 - p).powi(3);
        adj.set_value(start + (target - start) * ease);
        if p >= 1.0 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// Layered radial glow from the top-left cover position over a dark directional gradient,
/// plus a bright tint for the equalizer, song progress trace and active romanized lyric.
pub(crate) fn ambient_css(class: &str, (r, g, b): (u8, u8, u8)) -> String {
    let lift = |c: u8, num: u16, floor: u16| ((c as u16 * num / 110 + floor).min(255)) as u8;
    let scale = |c: u8, num: u16, den: u16| (c as u16 * num / den) as u8;
    let (gr, gg, gb) = (lift(r, 155, 18), lift(g, 155, 18), lift(b, 155, 18));
    let (hr, hg, hb) = (lift(r, 190, 65), lift(g, 190, 65), lift(b, 190, 65));
    format!(
        ".notch.{class} {{ background: radial-gradient(circle at 44px 48px, rgba({gr},{gg},{gb},0.62) 0%, rgba({r},{g},{b},0.28) 46%, transparent 78%), linear-gradient(160deg, {} 0%, rgb({},{},{}) 58%, rgb({},{},{}) 100%); }} \
         .notch.{class} .eq > box, .notch.{class} progressbar.song-trace progress {{ background: rgb({hr},{hg},{hb}); }} \
         .notch.{class} .lrc.now {{ border-left-color: rgb({hr},{hg},{hb}); }} \
         .notch.{class} .lrc.now .lrc-rom {{ color: rgb({hr},{hg},{hb}); }}",
        cfg().gradient[0],
        r / 4,
        g / 4,
        b / 4,
        scale(r, 3, 4),
        scale(g, 3, 4),
        scale(b, 3, 4),
    )
}

/// Center-crop a cover to a square and shrink it to 112px, 2x the largest size we draw.
/// Returns raw pixels (Pixbuf itself can't cross threads): (bytes, has_alpha, rowstride, accent rgb).
pub(crate) fn square_cover(bytes: &[u8]) -> Option<(glib::Bytes, bool, usize, (u8, u8, u8))> {
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    let pb = loader.pixbuf()?;
    let side = pb.width().min(pb.height());
    let sq = pb
        .new_subpixbuf(
            (pb.width() - side) / 2,
            (pb.height() - side) / 2,
            side,
            side,
        )
        .scale_simple(112, 112, gtk::gdk_pixbuf::InterpType::Bilinear)?;
    let (pixels, alpha, stride) = (
        sq.read_pixel_bytes(),
        sq.has_alpha(),
        sq.rowstride() as usize,
    );
    let accent = accent_color(&pixels, if alpha { 4 } else { 3 }, stride, 112);
    Some((pixels, alpha, stride, accent))
}

/// A dark, saturated version of the cover's main color, for the notch gradient.
/// Colorful pixels count more than gray ones so a mostly-white cover still gives a hue.
pub(crate) fn accent_color(px: &[u8], channels: usize, stride: usize, size: usize) -> (u8, u8, u8) {
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

/// Cover image bytes from an MPRIS artUrl (file:// from local players, https:// from browsers).
pub(crate) fn load_art(url: &str) -> Option<Vec<u8>> {
    if url.starts_with("http") {
        let mut resp = ureq::get(url).call().ok()?;
        resp.body_mut()
            .with_config()
            .limit(10 << 20)
            .read_to_vec()
            .ok()
    } else {
        std::fs::read(gio::File::for_uri(url).path()?).ok()
    }
}

#[test]
pub(crate) fn accent_is_dark_and_keeps_hue() {
    let red = [200u8, 20, 20].repeat(4);
    let (r, g, b) = accent_color(&red, 3, 6, 2);
    assert!(r == 110 && g < 20 && b < 20, "{r} {g} {b}");
    let css = ambient_css("tinted", (r, g, b));
    assert!(css.contains("radial-gradient") && css.contains("song-trace"));
}
