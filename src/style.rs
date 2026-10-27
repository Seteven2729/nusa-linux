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
.notch.open { border-radius: 0 0 28px 28px; padding: 8px 16px 14px; }
.notch.thin { padding: 0; min-height: 5px; box-shadow: none; }
.notch.music { min-width: 272px; }
.compact { font-size: 13px; font-weight: 600; }
.mini-art { border-radius: 5px; }
.art { border-radius: 12px; }
.title { font-size: 15px; font-weight: 700; }
.title.clock { font-size: 30px; font-weight: 700; font-feature-settings: 'tnum'; }
.artist { font-size: 12px; color: {MUTED}; }
.time { font-size: 11px; color: {MUTED}; font-feature-settings: 'tnum'; }
.lyric { font-size: 13px; font-weight: 400; color: #fff; opacity: 0.7; margin-top: 2px; }

.eq { min-height: 14px; }
.eq > box { background: {ACCENT}; border-radius: 2px; min-width: 3px; min-height: 3px; }
.dl-row { margin-top: 3px; }
.dl-row image { opacity: 0.7; }
progressbar.dl trough { min-height: 3px; min-width: 0; border-radius: 2px; background: rgba(255,255,255,0.22); border: none; }
progressbar.dl progress { min-height: 3px; border-radius: 2px; background: {ACCENT}; border: none; }

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
.notch button.page-arrow { min-width: 22px; min-height: 22px; opacity: 0.55; }
.notch button.page-arrow:hover { opacity: 1; }
.notch button.page-arrow:disabled { opacity: 0.15; }

@keyframes pop { from { transform: scale(1); } 40% { transform: scale(1.06); } to { transform: scale(1); } }
.notch.pop { transform-origin: 50% 0; animation: pop 320ms ease-out; }
@keyframes fadein { from { opacity: 0; } to { opacity: 1; } }
.compact.fade { animation: fadein 220ms ease-out; }

.notch button.panel-toggle { min-width: 30px; min-height: 30px; opacity: 0.6; }
.notch button.panel-toggle:checked { opacity: 1; background: alpha({HIGHLIGHT}, 0.2); }

.notch scrolledwindow, .notch list, .notch row { background: none; }
.notch row { padding: 0; border-radius: 6px; }
.notch row:hover { background: rgba(255,255,255,0.08); }
.lrc { font-size: 13px; padding: 3px 6px; opacity: 0.45; }
.lrc.now { opacity: 1; font-weight: 700; }
.rom { font-size: 11px; opacity: 0.6; }
.mic { color: #ff9f0a; font-size: 10px; }
.cam { color: #30d158; font-size: 10px; }
.share { color: #ff453a; font-size: 10px; }
.stats { font-size: 11px; color: {MUTED}; font-feature-settings: 'tnum'; }
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
