# Nusa for Linux

Notch kecil di tengah atas layar untuk desktop Linux berbasis Wayland. Terinspirasi dari [Nusa for Windows](https://rama2555.github.io/nusa/), ditulis ulang dari nol dengan Rust dan GTK4. Untuk pemakaian pribadi.

Hover notch untuk membukanya, geser kursor menjauh untuk menutupnya.

## Fitur

- Lagu yang sedang diputar dari player MPRIS mana pun (Spotify, Strawberry, browser, mpv): cover, judul, artis, prev/play/next, seek bar
- Lirik tersinkron dari [lrclib.net](https://lrclib.net), satu baris di bawah judul, atau panel lirik penuh (klik baris untuk lompat)
- Popup charger dicolok/dicabut, Wi-Fi tersambung/putus, headphone Bluetooth tersambung beserta baterainya
- Maskot mata saat idle: muncul setelah 30 detik, tertidur setelah 3 menit, tersenyum saat kamu kembali
- Jam besar dan tanggal (bahasa Indonesia) saat tidak ada lagu
- Menu tray: start with login, minimal notch (garis tipis), maskot, exit

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

## Update setelah mengubah kode

```sh
cd ~/nusa-linux
cargo install --path . --root ~/.local
pkill -x nusa-linux; ~/.local/bin/nusa-linux &
```

## Uninstall

```sh
pkill -x nusa-linux
rm ~/.local/bin/nusa-linux ~/.config/autostart/nusa-linux.desktop
rm -r ~/.config/nusa
```

## File yang dipakai

| Path | Isi |
|---|---|
| `~/.config/autostart/nusa-linux.desktop` | Autostart, dibuat/dihapus lewat tray |
| `~/.config/nusa/minimal` | Ada = mode minimal aktif |
| `~/.config/nusa/no-mascot` | Ada = maskot mati |

## Privasi

Satu-satunya data yang keluar dari komputer adalah judul, artis, dan durasi lagu yang dikirim ke lrclib.net untuk mencari lirik. Cover dari browser diunduh langsung dari URL yang diberikan player.

## Troubleshooting

- **Notch tidak muncul.** Jalankan `nusa-linux` dari terminal tanpa `&` dan lihat errornya. Pastikan sesi Wayland: `echo $XDG_SESSION_TYPE` harus `wayland`.
- **Lagu tidak terdeteksi.** Cek apakah player terdaftar di MPRIS: `busctl --user list | grep mpris`.
- **Muncul dua notch.** Tidak seharusnya terjadi, Nusa hanya mengizinkan satu instance. Jalankan `pkill -x nusa-linux` lalu buka lagi.

## Test

```sh
cargo test
```
