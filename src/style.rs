//! The notch stylesheet. `{TOKENS}` are theme colors, filled in from config.toml at startup.

use crate::*;

pub(crate) const CSS: &str = "
window { background: transparent; }
.notch {
  background: linear-gradient(160deg, {GRAD1} 0%, {GRAD2} 65%, {GRAD3} 100%); color: #f4f4f6;
  font-family: Inter, 'Noto Sans', sans-serif;
  border-radius: 0 0 14px 14px; padding: 5px 14px 7px; min-width: 150px;
  margin: 0 14px 14px; box-shadow: none;
  border: 1px solid rgba(255,255,255,0.07); border-top: none;
  transition: border-radius 180ms cubic-bezier(0.32, 0.72, 0, 1), padding 180ms cubic-bezier(0.32, 0.72, 0, 1), min-width 180ms cubic-bezier(0.32, 0.72, 0, 1);
}
.notch.open { border-radius: 0 0 18px 18px; padding: 4px 18px 16px; }
.notch.thin { padding: 0; min-height: 5px; box-shadow: none; border: none; }
.notch.music { min-width: 272px; }
.compact { font-size: 13px; font-weight: 600; letter-spacing: 0px; }
.mini-art { border-radius: 4px; }
.art { border-radius: 6px; box-shadow: none; }
.title { font-size: 15px; font-weight: 700; letter-spacing: -0.3px; }
.title.clock { font-size: 40px; font-weight: 800; letter-spacing: -1.5px; font-feature-settings: 'tnum'; }
.artist { font-size: 12px; color: {MUTED}; }
.time { font-size: 11px; color: {MUTED}; font-feature-settings: 'tnum'; }
.lyric { font-size: 13px; font-weight: 400; color: #fff; opacity: 0.7; margin-top: 2px; }

.eq { min-height: 14px; }
.eq > box { background: {ACCENT}; border-radius: 1px; min-width: 3px; min-height: 3px; }
.dl-row { margin-top: 3px; }
.dl-row image { opacity: 0.7; }
progressbar.dl trough { min-height: 3px; min-width: 0; border-radius: 1px; background: rgba(255,255,255,0.16); border: none; }
progressbar.dl progress { min-height: 3px; min-width: 0; border-radius: 1px; background: {ACCENT}; border: none; }
progressbar.song-trace { margin-top: 3px; }
progressbar.song-trace trough { min-height: 2px; min-width: 0; border-radius: 0; background: rgba(255,255,255,0.14); border: none; }
progressbar.song-trace progress { min-height: 2px; min-width: 0; border-radius: 0; background: {HIGHLIGHT}; border: none; }

.notch scale { padding: 0; min-width: 210px; }
.notch scale trough { min-height: 3px; border-radius: 1px; background: rgba(255,255,255,0.18); border: none; }
.notch scale highlight { background: {HIGHLIGHT}; border-radius: 1px; border: none; margin: 0; min-width: 0; }
.notch scale slider { min-width: 0; min-height: 0; margin: 0; background: {HIGHLIGHT}; border: none; box-shadow: none; transition: min-width 140ms cubic-bezier(0.23, 1, 0.32, 1), min-height 140ms cubic-bezier(0.23, 1, 0.32, 1), margin 140ms cubic-bezier(0.23, 1, 0.32, 1); }
.notch scale:hover slider { min-width: 10px; min-height: 10px; margin: -4px; }

.notch button { background: none; border: none; box-shadow: none; color: #fff; min-width: 34px; min-height: 34px; border-radius: 6px; padding: 0; transition: transform 140ms cubic-bezier(0.23, 1, 0.32, 1), background 140ms ease, opacity 140ms ease; }
.notch button:hover { background: rgba(255,255,255,0.1); }
.notch button:active { transform: scale(0.96); }
.notch button.play { background: {HIGHLIGHT}; color: {GRAD2}; border-radius: 8px; }
.notch button.play:hover { background: alpha({HIGHLIGHT}, 0.84); }
.section { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: {MUTED}; }
.notch button.player-switch { font-size: 11px; min-height: 22px; padding: 0 8px; border-radius: 4px; background: rgba(255,255,255,0.08); }
.notch button.player-switch:hover { background: rgba(255,255,255,0.15); }
.shot { border-radius: 4px; }
.notch button.shot-remove { min-width: 20px; min-height: 20px; margin: 4px; border-radius: 4px; background: rgba(0,0,0,0.65); opacity: 0.85; }
.no-media { font-size: 13px; color: {MUTED}; margin: 14px 24px; }
.page-nav { background: none; border-bottom: 1px solid rgba(255,255,255,0.1); border-radius: 0; padding: 0 0 4px; margin-bottom: 10px; }
.notch button.page-pill { min-width: 0; min-height: 22px; padding: 0 10px; border-radius: 4px; font-size: 11px; font-weight: 600; opacity: 0.48; }
.notch button.page-pill:hover { opacity: 0.88; background: rgba(255,255,255,0.06); }
.notch button.page-pill.active { opacity: 1; background: rgba(255,255,255,0.12); color: #fff; }

@keyframes pop { from { transform: scale(1); } 40% { transform: scale(1.035); } to { transform: scale(1); } }
.notch.pop { transform-origin: 50% 0; animation: pop 240ms cubic-bezier(0.23, 1, 0.32, 1); }
@keyframes fadein { from { opacity: 0; transform: translateY(2px); } to { opacity: 1; transform: translateY(0); } }
.compact.fade, .lyric.fade, .rom.fade { animation: fadein 200ms cubic-bezier(0.23, 1, 0.32, 1); }

.notch button.panel-toggle { min-width: 30px; min-height: 30px; border-radius: 6px; opacity: 0.58; }
.notch button.panel-toggle:checked { opacity: 1; background: rgba(255,255,255,0.14); }

.notch scrolledwindow, .notch list, .notch row { background: none; color: #fff; }
.notch scrollbar { opacity: 0; }
.notch row { padding: 0; border-radius: 4px; }
.notch row:hover { background: rgba(255,255,255,0.06); }
.lrc { padding: 4px 10px; border-radius: 0; border-left: 2px solid transparent; color: #fff; opacity: 0.28; transition: opacity 160ms ease, background 160ms ease; }
.lrc .lrc-main { font-size: 12px; font-weight: 500; color: #fff; }
.lrc .lrc-rom { font-size: 10px; color: {MUTED}; opacity: 0.75; }
.lrc.mid { opacity: 0.45; }
.lrc.near { opacity: 0.7; }
.lrc.near .lrc-main { font-size: 13px; }
.lrc.now { opacity: 1; background: rgba(255,255,255,0.06); border-left: 2px solid {ACCENT}; padding: 6px 10px; }
.lrc.now .lrc-main { font-size: 15px; font-weight: 700; }
.lrc.now .lrc-rom { font-size: 11px; font-weight: 600; color: {ACCENT}; opacity: 1; }
.rom { font-size: 11px; opacity: 0.6; }
.mic { color: #ff9f0a; font-size: 10px; }
.cam { color: #30d158; font-size: 10px; }
.share { color: #ff453a; font-size: 10px; }
.stats { font-size: 11px; color: {MUTED}; font-feature-settings: 'tnum'; }
.stat-lbl { font-size: 10px; font-weight: 600; letter-spacing: 0.3px; color: {MUTED}; font-feature-settings: 'tnum'; }
.stat-val { font-size: 11px; font-weight: 600; color: #f4f4f6; font-feature-settings: 'tnum'; }
.stat-val.hot { color: #ff9f0a; font-weight: 700; }
progressbar.stat-bar trough { min-height: 3px; min-width: 68px; border-radius: 1px; background: rgba(255,255,255,0.14); border: none; }
progressbar.stat-bar progress { min-height: 3px; min-width: 0; border-radius: 1px; background: {ACCENT}; border: none; }
progressbar.stat-bar.hot progress { background: #ff9f0a; }
.timer-badge { font-size: 11px; font-weight: 700; color: {HIGHLIGHT}; background: rgba(255,255,255,0.1); border-radius: 4px; padding: 1px 7px; font-feature-settings: 'tnum'; }
.timer-group { background: rgba(255,255,255,0.06); border-radius: 6px; padding: 2px 4px; }
.notch button.timer-step { min-width: 22px; min-height: 22px; border-radius: 4px; font-size: 13px; font-weight: 700; opacity: 0.75; }
.notch button.timer-step:hover { opacity: 1; background: rgba(255,255,255,0.12); }
.notch entry.timer-input { min-height: 22px; min-width: 48px; padding: 0 4px; background: transparent; border: none; box-shadow: none; outline: none; color: #fff; font-size: 12px; font-weight: 700; font-feature-settings: 'tnum'; }
.notch entry.timer-input:focus { background: rgba(255,255,255,0.12); border-radius: 4px; }
.notch button.timer-run { min-width: 24px; min-height: 22px; border-radius: 4px; font-size: 11px; background: rgba(255,255,255,0.12); }
.notch button.timer-run:hover { background: rgba(255,255,255,0.2); }
.notch button.timer-run.running { background: {HIGHLIGHT}; color: {GRAD2}; font-weight: 700; }
.notch button.clip { min-height: 26px; padding: 0 10px; border-radius: 6px; background: rgba(255,255,255,0.06); font-size: 12px; }
.notch button.clip:hover { background: rgba(255,255,255,0.12); }
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
