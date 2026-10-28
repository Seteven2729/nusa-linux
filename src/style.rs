//! The notch stylesheet. `{TOKENS}` are theme colors, filled in from config.toml at startup.

use crate::*;

pub(crate) const CSS: &str = "
window { background: transparent; }
.notch {
  background: linear-gradient(160deg, {GRAD1} 0%, {GRAD2} 65%, {GRAD3} 100%); color: #fff;
  font-family: Inter, 'Noto Sans', sans-serif;
  border-radius: 0 0 20px 20px; padding: 5px 14px 7px; min-width: 150px;
  margin: 0 14px 14px; box-shadow: 0 6px 20px rgba(4,12,40,0.6);
  transition: border-radius 180ms ease, padding 180ms ease, min-width 180ms ease-out;
}
.notch.open { border-radius: 0 0 28px 28px; padding: 6px 16px 14px; }
.notch.thin { padding: 0; min-height: 5px; box-shadow: none; }
.notch.music { min-width: 272px; }
.compact { font-size: 13px; font-weight: 600; }
.mini-art { border-radius: 5px; }
.art { border-radius: 12px; box-shadow: 0 4px 12px rgba(0,0,0,0.45); }
.title { font-size: 15px; font-weight: 700; }
.title.clock { font-size: 38px; font-weight: 800; letter-spacing: -1px; font-feature-settings: 'tnum'; }
.artist { font-size: 12px; color: {MUTED}; }
.time { font-size: 11px; color: {MUTED}; font-feature-settings: 'tnum'; }
.lyric { font-size: 13px; font-weight: 400; color: #fff; opacity: 0.7; margin-top: 2px; }

.eq { min-height: 14px; }
.eq > box { background: {ACCENT}; border-radius: 2px; min-width: 3px; min-height: 3px; }
.dl-row { margin-top: 3px; }
.dl-row image { opacity: 0.7; }
progressbar.dl trough { min-height: 3px; min-width: 0; border-radius: 2px; background: rgba(255,255,255,0.22); border: none; }
progressbar.dl progress { min-height: 3px; min-width: 0; border-radius: 2px; background: {ACCENT}; border: none; }
progressbar.song-trace { margin-top: 3px; }
progressbar.song-trace trough { min-height: 2px; min-width: 0; border-radius: 99px; background: rgba(255,255,255,0.14); border: none; }
progressbar.song-trace progress { min-height: 2px; min-width: 0; border-radius: 99px; background: {HIGHLIGHT}; border: none; }

.notch scale { padding: 0; min-width: 210px; }
.notch scale trough { min-height: 4px; border-radius: 2px; background: rgba(255,255,255,0.22); border: none; }
.notch scale highlight { background: {HIGHLIGHT}; border-radius: 2px; border: none; margin: 0; min-width: 0; }
.notch scale slider { min-width: 0; min-height: 0; margin: 0; background: {HIGHLIGHT}; border: none; box-shadow: none; transition: all 120ms ease; }
.notch scale:hover slider { min-width: 12px; min-height: 12px; margin: -4px; }

.notch button { background: none; border: none; box-shadow: none; color: #fff; min-width: 36px; min-height: 36px; border-radius: 999px; padding: 0; }
.notch button:hover { background: rgba(255,255,255,0.15); }
.notch button.play { background: {HIGHLIGHT}; color: {GRAD2}; }
.notch button.play:hover { background: alpha({HIGHLIGHT}, 0.82); }
.section { font-size: 11px; font-weight: 700; color: {MUTED}; }
.notch button.player-switch { font-size: 11px; min-height: 22px; padding: 0 10px; border-radius: 999px; background: rgba(255,255,255,0.08); }
.notch button.player-switch:hover { background: rgba(255,255,255,0.16); }
.shot { border-radius: 8px; }
.notch button.shot-remove { min-width: 20px; min-height: 20px; margin: 4px; background: rgba(0,0,0,0.55); opacity: 0.8; }
.no-media { font-size: 13px; color: {MUTED}; margin: 14px 24px; }
.page-nav { background: rgba(255,255,255,0.06); border-radius: 999px; padding: 2px; margin-bottom: 6px; }
.notch button.page-pill { min-width: 0; min-height: 20px; padding: 0 10px; border-radius: 999px; font-size: 11px; font-weight: 600; opacity: 0.52; }
.notch button.page-pill:hover { opacity: 0.88; background: rgba(255,255,255,0.08); }
.notch button.page-pill.active { opacity: 1; background: rgba(255,255,255,0.18); color: #fff; }

@keyframes pop { from { transform: scale(1); } 40% { transform: scale(1.06); } to { transform: scale(1); } }
.notch.pop { transform-origin: 50% 0; animation: pop 320ms ease-out; }
@keyframes fadein { from { opacity: 0; } to { opacity: 1; } }
.compact.fade { animation: fadein 220ms ease-out; }

.notch button.panel-toggle { min-width: 30px; min-height: 30px; opacity: 0.6; }
.notch button.panel-toggle:checked { opacity: 1; background: alpha({HIGHLIGHT}, 0.2); }

.notch scrolledwindow, .notch list, .notch row { background: none; }
.notch row { padding: 0; border-radius: 8px; }
.notch row:hover { background: rgba(255,255,255,0.08); }
.lrc { padding: 4px 8px; border-radius: 8px; opacity: 0.28; transition: opacity 160ms ease, background 160ms ease; }
.lrc .lrc-main { font-size: 12px; font-weight: 500; }
.lrc .lrc-rom { font-size: 10px; color: {MUTED}; opacity: 0.75; }
.lrc.mid { opacity: 0.45; }
.lrc.near { opacity: 0.7; }
.lrc.near .lrc-main { font-size: 13px; }
.lrc.now { opacity: 1; background: rgba(255,255,255,0.08); padding: 6px 8px; }
.lrc.now .lrc-main { font-size: 15px; font-weight: 700; }
.lrc.now .lrc-rom { font-size: 11px; font-weight: 600; color: {ACCENT}; opacity: 1; }
.rom { font-size: 11px; opacity: 0.6; }
.mic { color: #ff9f0a; font-size: 10px; }
.cam { color: #30d158; font-size: 10px; }
.share { color: #ff453a; font-size: 10px; }
.stats { font-size: 11px; color: {MUTED}; font-feature-settings: 'tnum'; }
.stat-lbl { font-size: 10px; font-weight: 700; color: {MUTED}; font-feature-settings: 'tnum'; }
.stat-val { font-size: 11px; font-weight: 600; color: {MUTED}; font-feature-settings: 'tnum'; }
.stat-val.hot { color: #ff9f0a; font-weight: 700; }
progressbar.stat-bar trough { min-height: 4px; min-width: 68px; border-radius: 99px; background: rgba(255,255,255,0.14); border: none; }
progressbar.stat-bar progress { min-height: 4px; min-width: 0; border-radius: 99px; background: {ACCENT}; border: none; }
progressbar.stat-bar.hot progress { background: #ff9f0a; }
.timer-badge { font-size: 11px; font-weight: 700; color: {HIGHLIGHT}; background: rgba(255,255,255,0.12); border-radius: 999px; padding: 1px 7px; font-feature-settings: 'tnum'; }
.timer-group { background: rgba(255,255,255,0.06); border-radius: 999px; padding: 2px 4px; }
.notch button.timer-step { min-width: 22px; min-height: 22px; border-radius: 999px; font-size: 13px; font-weight: 700; opacity: 0.75; }
.notch button.timer-step:hover { opacity: 1; background: rgba(255,255,255,0.14); }
.notch entry.timer-input { min-height: 22px; min-width: 48px; padding: 0 4px; background: transparent; border: none; box-shadow: none; outline: none; color: #fff; font-size: 12px; font-weight: 700; font-feature-settings: 'tnum'; }
.notch entry.timer-input:focus { background: rgba(255,255,255,0.12); border-radius: 6px; }
.notch button.timer-run { min-width: 24px; min-height: 22px; border-radius: 999px; font-size: 11px; background: rgba(255,255,255,0.12); }
.notch button.timer-run:hover { background: rgba(255,255,255,0.22); }
.notch button.timer-run.running { background: {HIGHLIGHT}; color: {GRAD2}; font-weight: 700; }
.notch button.clip { min-height: 26px; padding: 0 10px; border-radius: 8px; background: rgba(255,255,255,0.06); font-size: 12px; }
.notch button.clip:hover { background: rgba(255,255,255,0.14); }
";

/// The stylesheet with its {TOKENS} replaced by the theme's colors.
pub(crate) fn themed_css(c: &Config) -> String {
    let mut css = CSS.to_owned();
    for (token, color) in [
        ("{GRAD1}", &c.gradient[0]),
        ("{GRAD2}", &c.gradient[1]),
        ("{GRAD3}", &c.gradient[2]),
        ("{MUTED}", &c.muted),
        ("{ACCENT}", &c.accent),
        ("{HIGHLIGHT}", &c.highlight),
    ] {
        css = css.replace(token, color);
    }
    css
}
