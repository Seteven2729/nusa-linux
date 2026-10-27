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
    let props = MessageStream::for_match_rule(rule, &conn, None).await?.map(|_| ());
    let owners = dbus
        .receive_name_owner_changed()
        .await?
        .filter(|s| std::future::ready(s.args().is_ok_and(|a| a.name().starts_with("org.mpris.MediaPlayer2."))))
        .map(|_| ());
    // The player switch button asks for a re-pick through this channel.
    let (repick_tx, repick_rx) = async_channel::unbounded::<()>();
    state.borrow_mut().repick = Some(repick_tx);
    // async_channel's Receiver isn't Unpin, so pin the merged stream to poll it.
    let mut events = std::pin::pin!(futures_util::stream::select(futures_util::stream::select(props, owners), repick_rx));

    // Re-pick from every live player on each event instead of tracking incrementally,
    // so closed players and failed calls can never leave a stale choice behind.
    loop {
        let current = state.borrow().player.as_ref().map(|p| p.inner().destination().to_string());
        let pinned = state.borrow().pinned.clone();
        let (picked, players) = pick_player(&conn, &dbus, current.as_deref(), pinned.as_deref()).await;
        {
            let mut st = state.borrow_mut();
            // A pinned player that closed releases the pin; picking goes back to automatic.
            if st.pinned.as_ref().is_some_and(|p| !players.iter().any(|(o, _)| o == p)) {
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
        let Some(suffix) = name.strip_prefix("org.mpris.MediaPlayer2.") else { continue };
        let Ok(owner) = dbus.get_name_owner(name.as_ref()).await else { continue };
        let Ok(p) = player(conn, owner.to_string()).await else { continue };
        let Ok(status) = p.playback_status().await else { continue };
        let identity = match MediaPlayerProxy::builder(conn).destination(owner.to_string()).build().await {
            Ok(mp) => mp.identity().await.ok(),
            Err(_) => None,
        };
        // "firefox.instance_1_23" -> "firefox" when the player has no Identity.
        let label = identity.unwrap_or_else(|| suffix.split('.').next().unwrap_or(suffix).to_owned());
        alive.push((owner.to_string(), label, status == "Playing", p));
    }
    let is = |o: &str, want: Option<&str>| Some(o) == want;
    let idx = alive
        .iter()
        .position(|(o, ..)| is(o, pinned))
        .or_else(|| alive.iter().position(|(o, _, playing, _)| *playing && is(o, current)))
        .or_else(|| alive.iter().position(|(_, _, playing, _)| *playing))
        .or_else(|| alive.iter().position(|(o, ..)| is(o, current)))
        .or((!alive.is_empty()).then_some(0));
    let players = alive.iter().map(|(o, l, ..)| (o.clone(), l.clone())).collect();
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
    let Some(p) = state.borrow().player.clone() else { return };
    let (Ok(meta), Ok(status)) = (p.metadata().await, p.playback_status().await) else {
        // Player went away.
        clear_player(ui, state);
        return;
    };
    let str_of = |k: &str| meta.get(k).and_then(|v| String::try_from(v.try_clone().ok()?).ok()).unwrap_or_default();
    let title = str_of("xesam:title");
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok())
        .map(|a| a.join(", "))
        .unwrap_or_default();
    let length_s = meta
        .get("mpris:length")
        .and_then(|v| i64::try_from(v.try_clone().ok()?).ok().or_else(|| u64::try_from(v.try_clone().ok()?).ok().map(|n| n as i64)))
        .unwrap_or(0)
        / 1_000_000;

    let playing = status == "Playing";
    let art_url = str_of("mpris:artUrl");
    if state.borrow().art_url != art_url {
        state.borrow_mut().art_url = art_url.clone();
        let (ui, state) = (ui.clone(), state.clone());
        glib::spawn_future_local(async move {
            let url = art_url.clone();
            let cover = gio::spawn_blocking(move || square_cover(&load_art(&url)?)).await.ok().flatten();
            if let Some((_, _, _, (r, g, b))) = &cover {
                ui.accent.load_from_string(&format!(
                    ".notch.tinted {{ background: linear-gradient(160deg, {} 0%, rgb({},{},{}) 65%, rgb({r},{g},{b}) 100%); }}",
                    cfg().gradient[0],
                    r / 3,
                    g / 3,
                    b / 3
                ));
            }
            if state.borrow().art_url != art_url {
                return; // track changed while loading
            }
            let tex = cover.map(|(pixels, alpha, stride, _)| {
                let format = if alpha { gtk::gdk::MemoryFormat::R8g8b8a8 } else { gtk::gdk::MemoryFormat::R8g8b8 };
                gtk::gdk::MemoryTexture::new(112, 112, format, &pixels, stride).upcast::<gtk::gdk::Texture>()
            });
            ui.art.set_paintable(tex.as_ref());
            ui.mini_art.set_paintable(tex.as_ref());
            ui.art.set_visible(tex.is_some());
            refresh_compact(&ui, &state.borrow());
        });
    }
    state.borrow_mut().track_id = meta.get("mpris:trackid").and_then(|v| OwnedObjectPath::try_from(v.try_clone().ok()?).ok());
    ui.seek.set_range(0.0, length_s.max(1) as f64);
    ui.seek.parent().unwrap().set_visible(length_s > 0);
    ui.title.set_label(&title);
    ui.artist.set_label(&artist);
    ui.play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });

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
                let lyrics = if cfg().lyrics { fetch_lyrics(&t, &artist, length_s) } else { vec![] };
                let rom = if cfg().romanize { romanize_song(&lyrics) } else { vec![None; lyrics.len()] };
                (lyrics, rom)
            })
            .await
            .unwrap_or_default();
            if state.borrow().track == title {
                for ((_, line), r) in lyrics.iter().zip(&rom) {
                    let text = if line.is_empty() { "♪".to_string() } else { glib::markup_escape_text(line).to_string() };
                    let markup = match r {
                        Some(r) => format!("{text}\n<span size='small' alpha='60%'>{}</span>", glib::markup_escape_text(r)),
                        None => text,
                    };
                    let label = gtk::Label::builder()
                        .label(markup)
                        .use_markup(true)
                        .css_classes(["lrc"])
                        .xalign(0.0)
                        .wrap(true)
                        .max_width_chars(30)
                        .build();
                    ui.lyrics_list.append(&label);
                }
                state.borrow_mut().lyrics = lyrics;
                state.borrow_mut().lyrics_rom = rom;
                refresh_compact(&ui, &state.borrow());
            }
        });
    }
}

/// Light up line `idx` in the full lyrics list and scroll it to the middle.
pub(crate) fn highlight_lyric(ui: &Ui, st: &mut State, idx: Option<usize>) {
    if st.lyric_idx == idx {
        return;
    }
    let row = |i: Option<usize>| i.and_then(|i| ui.lyrics_list.row_at_index(i as i32)).and_then(|r| r.child());
    if let Some(old) = row(st.lyric_idx) {
        old.remove_css_class("now");
    }
    if let Some(new) = row(idx) {
        new.add_css_class("now");
        if let Some(b) = new.compute_bounds(&ui.lyrics_list) {
            let adj = ui.lyrics_scroll.vadjustment();
            adj.set_value((b.y() as f64 + b.height() as f64 / 2.0 - adj.page_size() / 2.0).max(0.0));
        }
    }
    st.lyric_idx = idx;
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
        .new_subpixbuf((pb.width() - side) / 2, (pb.height() - side) / 2, side, side)
        .scale_simple(112, 112, gtk::gdk_pixbuf::InterpType::Bilinear)?;
    let (pixels, alpha, stride) = (sq.read_pixel_bytes(), sq.has_alpha(), sq.rowstride() as usize);
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
        resp.body_mut().with_config().limit(10 << 20).read_to_vec().ok()
    } else {
        std::fs::read(gio::File::for_uri(url).path()?).ok()
    }
}

#[test]
pub(crate) fn accent_is_dark_and_keeps_hue() {
    let red = [200u8, 20, 20].repeat(4);
    let (r, g, b) = accent_color(&red, 3, 6, 2);
    assert!(r == 110 && g < 20 && b < 20, "{r} {g} {b}");
}
