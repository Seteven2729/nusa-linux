# Nusa for Linux

Notch kecil di tengah atas layar untuk desktop Linux berbasis Wayland. Terinspirasi dari [Nusa for Windows](https://rama2555.github.io/nusa/), ditulis ulang dari nol dengan Rust dan GTK4. Untuk pemakaian pribadi.

Hover notch untuk membukanya, geser kursor menjauh untuk menutupnya.

## Fitur

- Lagu yang sedang diputar dari player MPRIS mana pun (Spotify, Strawberry, browser, mpv): cover, judul, artis, prev/play/next, seek bar
- Warna gradien notch mengikuti warna cover lagu, dan saat idle mengikuti warna wallpaper Plasma
- Lirik tersinkron dari [lrclib.net](https://lrclib.net), satu baris di bawah judul, atau panel lirik penuh (klik baris untuk lompat)
- Romanisasi lirik untuk Korea, Jepang (termasuk kanji), Mandarin (pinyin), dan Cyrillic
- Popup charger dicolok/dicabut, Wi-Fi tersambung/putus, headphone Bluetooth tersambung beserta baterainya
- Titik oranye saat ada aplikasi yang memakai mikrofon
- Progres download (browser dan aplikasi lain yang memakai Unity LauncherEntry): bar tipis di notch, lalu popup saat selesai
- Notifikasi HP lewat KDE Connect, termasuk telepon masuk
- Maskot mata saat idle: muncul setelah 30 detik, tertidur setelah 3 menit, tersenyum saat kamu kembali. Kalau musik sedang jalan, maskot tersenyum dan mengangguk-angguk. Kalau di-pause, matanya mengantuk.
- Saat tidak ada lagu: jam besar, tanggal (bahasa Indonesia), dan CPU/RAM/suhu
- Tombol clipboard di tampilan idle: membuka 3 isi clipboard terakhir (klik untuk copy ulang). Panel tertutup lagi setiap notch ditutup.
- Menu tray: start with login, minimal notch (garis tipis), maskot, edit settings, reload, exit
- Hidup lagi sendiri kalau crash (systemd user service)

## Gesture

Berlaku langsung di notch, tanpa perlu membukanya.

| Gesture | Aksi |
|---|---|
| Scroll atas/bawah (mouse atau dua jari) | Volume ±5% |
| Geser dua jari kiri/kanan | Lagu sebelumnya/berikutnya |
| Klik tengah | Play/pause |

## Kebutuhan

Desktop harus berjalan di Wayland dengan dukungan `wlr-layer-shell`.

| Desktop | Status |
|---|---|
| KDE Plasma (Wayland) | Semua fitur jalan |
| Hyprland, Sway, niri, COSMIC | Jalan. Tray icon butuh bar dengan system tray (mis. Waybar) |
| GNOME | Tidak didukung, GNOME tidak punya layer-shell |
| Sesi X11 | Tidak didukung |

Layanan berikut opsional. Kalau tidak ada, fiturnya diam dan sisanya tetap jalan:

- UPower untuk popup charger
- NetworkManager untuk popup Wi-Fi
- BlueZ untuk popup headphone
- `ext-idle-notify` di compositor untuk maskot
- `wpctl` (WirePlumber) untuk gesture volume, `pactl` untuk indikator mikrofon
- Klipper (bawaan KDE) untuk daftar clipboard
- KDE Connect dengan HP yang sudah di-pair untuk notifikasi HP

Font yang dipakai Inter, dengan cadangan Noto Sans.

## Install

### 1. Pasang Rust

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Atau lewat package manager distro (`rust` di Arch).

### 2. Pasang library GTK4 dan gtk4-layer-shell

```sh
# Arch / Manjaro / EndeavourOS
sudo pacman -S gtk4 gtk4-layer-shell

# Fedora
sudo dnf install gtk4-devel gtk4-layer-shell-devel

# Ubuntu 24.04+ / Debian 13+
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev
```

### 3. Build dan pasang

```sh
git clone https://github.com/Seteven2729/nusa-linux.git ~/nusa-linux
cd ~/nusa-linux
cargo install --path . --root ~/.local
```

Binary terpasang di `~/.local/bin/nusa-linux`.

### 4. Jalankan

```sh
~/.local/bin/nusa-linux &
```

Untuk autostart saat login, klik ikon Nusa di system tray dan centang `Start with login`. Ikonnya berupa pil putih. Di KDE, kalau tidak terlihat, cek panah `^` di tray.

`Start with login` memasang systemd user service `nusa-linux.service`. Selain jalan otomatis saat login, service ini menyalakan Nusa lagi 2 detik setelah crash. Exit dari tray tidak dianggap crash, jadi Nusa tetap mati. Status dan log:

```sh
systemctl --user status nusa-linux.service
journalctl --user -u nusa-linux.service
```

## Setting

Semua setting ada di `~/.config/nusa/config.toml`, yang dibuat otomatis saat pertama kali jalan. Setiap baris ada penjelasannya. Buka lewat tray, `Edit settings`, lalu pilih `Reload` untuk menerapkan perubahan.

Yang bisa diatur:

- Fitur mana saja yang aktif: lirik, romanisasi, gesture, statistik, clipboard, mikrofon, notifikasi HP, download, baterai, Wi-Fi, Bluetooth
- Berapa detik idle sebelum maskot muncul dan tertidur
- Warna gradien notch, dan apakah warnanya ikut cover lagu atau wallpaper

Nilai yang salah atau hilang memakai default, jadi file yang rusak tidak membuat Nusa gagal jalan.

## Update setelah mengubah kode

```sh
cd ~/nusa-linux
cargo install --path . --root ~/.local
systemctl --user restart nusa-linux.service
```

Kalau tidak memakai `Start with login`, ganti baris terakhir dengan `pkill -x nusa-linux; ~/.local/bin/nusa-linux &`.

## Uninstall

```sh
systemctl --user disable --now nusa-linux.service
pkill -x nusa-linux
rm ~/.local/bin/nusa-linux ~/.config/systemd/user/nusa-linux.service
rm -r ~/.config/nusa ~/.cache/nusa
```

## File yang dipakai

| Path | Isi |
|---|---|
| `~/.config/nusa/config.toml` | Semua setting |
| `~/.config/systemd/user/nusa-linux.service` | Autostart dan restart otomatis, dipasang lewat tray |
| `~/.cache/nusa/lyrics/` | Lirik yang sudah pernah diambil, supaya langsung muncul di pemutaran berikutnya |

## Privasi

Satu-satunya data yang keluar dari komputer adalah judul, artis, dan durasi lagu yang dikirim ke lrclib.net untuk mencari lirik. Cover dari browser diunduh langsung dari URL yang diberikan player.

Isi clipboard hanya tampil setelah tombol clipboard diklik. Notifikasi HP muncul 5 detik tanpa perlu hover, jadi perhatikan saat berbagi layar.

## Troubleshooting

- **Notch tidak muncul.** Jalankan `nusa-linux` dari terminal tanpa `&` dan lihat errornya. Pastikan sesi Wayland: `echo $XDG_SESSION_TYPE` harus `wayland`.
- **Lagu tidak terdeteksi.** Cek apakah player terdaftar di MPRIS: `busctl --user list | grep mpris`.
- **Titik mikrofon tidak muncul.** Cek apakah ada stream yang aktif: `pactl list source-outputs short`. Titik hanya tampil saat notch tertutup.
- **Nusa mati terus setelah crash.** systemd berhenti mencoba kalau crash berulang dalam waktu singkat. Lihat penyebabnya di `journalctl --user -u nusa-linux.service`, lalu `systemctl --user reset-failed nusa-linux.service` dan start lagi.
- **Muncul dua notch.** Tidak seharusnya terjadi, Nusa hanya mengizinkan satu instance. Jalankan `pkill -x nusa-linux` lalu buka lagi.

## Lisensi

GPL-3.0-or-later, lihat [LICENSE](LICENSE). Romanisasi kanji memakai [kakasi](https://crates.io/crates/kakasi), yang berlisensi GPL-3, sehingga program ini juga GPL-3.

Bacaan kanji diambil dari kamus tanpa melihat konteks kalimat, jadi kadang meleset. Beberapa kata yang paling sering muncul di lirik (君, 愛して, partikel は setelah kata ganti) dikoreksi manual.

## Test

```sh
cargo test
```
