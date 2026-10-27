//! Screenshots page: the newest captures from ~/Pictures/Screenshots as thumbnails you can
//! drag into other apps, open, or drop from the row, plus a short preview when a new one lands.

use crate::*;

/// How many screenshots the page keeps.
const KEEP: usize = 5;

pub(crate) fn screenshots_dir() -> PathBuf {
    glib::user_special_dir(glib::UserDirectory::Pictures).unwrap_or_else(|| glib::home_dir().join("Pictures")).join("Screenshots")
}

fn is_image(p: &std::path::Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref(), Some("png" | "jpg" | "jpeg" | "webp"))
}

/// Newest images in `dir`, newest first.
fn newest_images(dir: &std::path::Path, n: usize) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        // Dotfiles are usually temp files mid-save.
        .filter(|p| is_image(p) && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(n).map(|(_, p)| p).collect()
}

/// A thumbnail at most 150x90, decoded off the main thread (Pixbuf can't cross threads,
/// so raw pixels do): (bytes, has_alpha, rowstride, width, height).
fn thumbnail(path: &std::path::Path) -> Option<(glib::Bytes, bool, usize, i32, i32)> {
    let pb = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, 150, 90, true).ok()?;
    Some((pb.read_pixel_bytes(), pb.has_alpha(), pb.rowstride() as usize, pb.width(), pb.height()))
}

fn texture((pixels, alpha, stride, w, h): (glib::Bytes, bool, usize, i32, i32)) -> gtk::gdk::Texture {
    let format = if alpha { gtk::gdk::MemoryFormat::R8g8b8a8 } else { gtk::gdk::MemoryFormat::R8g8b8 };
    gtk::gdk::MemoryTexture::new(w, h, format, &pixels, stride).upcast()
}

/// Rebuild the thumbnail row from `state.shots`.
fn fill_shots_page(ui: &Ui, state: &Shared) {
    let (ui, state) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        let paths = state.borrow().shots.clone();
        let p = paths.clone();
        let Ok(thumbs) = gio::spawn_blocking(move || p.iter().map(|p| thumbnail(p)).collect::<Vec<_>>()).await else { return };
        while let Some(c) = ui.shots_row.first_child() {
            ui.shots_row.remove(&c);
        }
        if paths.is_empty() {
            let none = tr("Belum ada screenshot di Pictures/Screenshots", "No screenshots in Pictures/Screenshots yet");
            ui.shots_row.append(&gtk::Label::builder().label(none).css_classes(["time"]).build());
        }
        for (path, thumb) in paths.into_iter().zip(thumbs) {
            let Some(thumb) = thumb else { continue };
            let tex = texture(thumb);
            let pic = gtk::Picture::builder().paintable(&tex).css_classes(["shot"]).cursor(&gtk::gdk::Cursor::from_name("pointer", None).unwrap()).build();
            pic.set_tooltip_text(path.file_name().and_then(|n| n.to_str()));
            // Drag the file into a chat, browser upload or folder.
            let drag = gtk::DragSource::new();
            let file = gio::File::for_path(&path);
            drag.set_content(Some(&gtk::gdk::ContentProvider::for_value(&gtk::gdk::FileList::from_array(&[file.clone()]).to_value())));
            let t = tex.clone();
            drag.connect_drag_begin(move |d, _| d.set_icon(Some(&t), 0, 0));
            pic.add_controller(drag);
            // Click opens it in the image viewer.
            let click = gtk::GestureClick::new();
            let f = file.clone();
            click.connect_released(move |_, _, _, _| {
                let _ = gio::AppInfo::launch_default_for_uri(&f.uri(), None::<&gio::AppLaunchContext>);
            });
            pic.add_controller(click);
            // × hides it from the row only; the file stays.
            let remove = gtk::Button::builder().icon_name("window-close-symbolic").css_classes(["shot-remove"]).halign(gtk::Align::End).valign(gtk::Align::Start).build();
            let (u, s, p) = (ui.clone(), state.clone(), path.clone());
            remove.connect_clicked(move |_| {
                s.borrow_mut().shots.retain(|x| *x != p);
                fill_shots_page(&u, &s);
            });
            let tile = gtk::Overlay::builder().child(&pic).build();
            tile.add_overlay(&remove);
            ui.shots_row.append(&tile);
        }
    });
}

/// Seed the page from the folder, then follow it with inotify: each finished image file
/// goes to the front of the row and pops a preview in the notch.
pub(crate) fn spawn_screenshot_watch(ui: &Ui, state: &Shared) {
    let dir = screenshots_dir();
    state.borrow_mut().shots = newest_images(&dir, KEEP);
    fill_shots_page(ui, state);
    let (tx, rx) = async_channel::unbounded::<PathBuf>();
    std::thread::spawn(move || {
        let _ = std::fs::create_dir_all(&dir);
        // SAFETY: plain inotify syscalls on a descriptor owned by this thread.
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        let Ok(cdir) = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()) else { return };
        // CLOSE_WRITE: written in place; MOVED_TO: written elsewhere and renamed in.
        if fd < 0 || unsafe { libc::inotify_add_watch(fd, cdir.as_ptr(), libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO) } < 0 {
            return;
        }
        let mut buf = [0u8; 4096];
        loop {
            let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                return;
            }
            // Walk the inotify_event records: wd, mask, cookie, len, then a NUL-padded name.
            let mut off = 0;
            while off + 16 <= n as usize {
                let len = u32::from_ne_bytes(buf[off + 12..off + 16].try_into().unwrap()) as usize;
                let name = &buf[off + 16..off + 16 + len];
                let name = String::from_utf8_lossy(name.split(|b| *b == 0).next().unwrap_or_default()).into_owned();
                let path = dir.join(&name);
                if !name.is_empty() && !name.starts_with('.') && is_image(&path) && tx.send_blocking(path).is_err() {
                    return;
                }
                off += 16 + len;
            }
        }
    });
    let (ui, state) = (ui.clone(), state.clone());
    glib::spawn_future_local(async move {
        while let Ok(path) = rx.recv().await {
            {
                let mut st = state.borrow_mut();
                st.shots.retain(|p| *p != path);
                st.shots.insert(0, path.clone());
                st.shots.truncate(KEEP);
            }
            fill_shots_page(&ui, &state);
            // Preview: the thumbnail rides along with a short alert in the closed notch.
            let p = path.clone();
            let thumb = gio::spawn_blocking(move || thumbnail(&p)).await.ok().flatten();
            show_alert_for(&ui, &state, tr("📸  Screenshot tersimpan", "📸  Screenshot saved").into(), 4);
            state.borrow_mut().alert_thumb = thumb.map(texture);
            refresh_compact(&ui, &state.borrow());
        }
    });
}

#[test]
fn newest_screenshots_first() {
    let dir = std::env::temp_dir().join(format!("nusa-shots-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["a.png", "notes.txt", "b.jpg", ".hidden.png", "c.PNG"] {
        std::thread::sleep(Duration::from_millis(15));
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    let names: Vec<_> = newest_images(&dir, 2).iter().map(|p| p.file_name().unwrap().to_owned()).collect();
    assert_eq!(names, ["c.PNG", "b.jpg"], "newest first, non-images and dotfiles skipped, capped at n");
    std::fs::remove_dir_all(dir).unwrap();
}
