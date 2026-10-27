#!/bin/sh
# Screenshot the notch in a given state, using only fake data.
#
#   tools/shot.sh <out.png> "<NUSA_DEMO spec>" [--music] [--theme <name>]
#   tools/shot.sh /tmp/lyrics.png "page=media,lyrics" --music
#
# NUSA_DEMO options are documented on `fn demo` in src/main.rs. The run uses a temporary
# config and temporary Pictures/Downloads folders, a fake player (--music) filtered with
# NUSA_ONLY_PLAYER, and a plain backdrop. The user's running Nusa is stopped for the shot
# and started again afterwards.
set -eu
out=$1
spec=$2
shift 2
music=0
theme=nusa
while [ $# -gt 0 ]; do
    case $1 in
        --music) music=1 ;;
        --theme) theme=$2; shift ;;
    esac
    shift
done

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
pids=""
was_active=0
systemctl --user is-active -q nusa-linux.service && was_active=1

cleanup() {
    for p in $pids; do kill "$p" 2>/dev/null || true; done
    pkill -f "$root/target/debug/nusa-linux" 2>/dev/null || true
    rm -rf "$tmp"
    [ "$was_active" = 1 ] && systemctl --user start nusa-linux.service
}
trap cleanup EXIT

cargo build --manifest-path "$root/Cargo.toml" -q
mkdir -p "$tmp/config/nusa" "$tmp/pictures/Screenshots" "$tmp/downloads"
# A theme is only visible without the cover/wallpaper tint on top of it.
follow=true
[ "$theme" != nusa ] && follow=false
printf '[colors]\ntheme = "%s"\nfollow_cover = %s\nfollow_wallpaper = %s\n' "$theme" "$follow" "$follow" > "$tmp/config/nusa/config.toml"
printf 'XDG_PICTURES_DIR="%s"\nXDG_DOWNLOAD_DIR="%s"\n' "$tmp/pictures" "$tmp/downloads" > "$tmp/config/user-dirs.dirs"

[ "$was_active" = 1 ] && systemctl --user stop nusa-linux.service
python3 -I "$root/tools/backdrop.py" >/dev/null 2>&1 & pids="$pids $!"
if [ "$music" = 1 ]; then
    magick -size 300x300 gradient:'#ff2d55-#5e5ce6' "$tmp/cover.png"
    python3 -I "$root/tools/fake_player.py" --title "Let's not fall in love" --artist BIGBANG \
        --length 211 --cover "$tmp/cover.png" --identity Spotify & pids="$pids $!"
fi
sleep 1.5

XDG_CONFIG_HOME="$tmp/config" NUSA_ONLY_PLAYER=fake NUSA_DEMO="$spec" \
    "$root/target/debug/nusa-linux" >/dev/null 2>&1 & pids="$pids $!"
sleep 7

spectacle -b -n -f -o "$tmp/full.png" 2>/dev/null
# Top-centre of a 1920-wide screen, trimmed to the notch, attached to the top edge.
magick "$tmp/full.png" -crop 1100x360+410+0 +repage -fuzz 4% -trim +repage \
    -bordercolor '#cfd4dc' -border 30 -gravity north -chop 0x30 +repage -strip "$out"
echo "$out"
