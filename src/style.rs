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
.row-label { font-size: 12px; font-weight: 500; }
.mini-art { border-radius: 4px; }
.art { border-radius: 6px; box-shadow: none; }
.title { font-size: 15px; font-weight: 700; letter-spacing: -0.3px; }
.title.clock { font-size: 40px; font-weight: 800; letter-spacing: -1.5px; line-height: 0.95; font-feature-settings: 'tnum'; }
.artist { font-size: 12px; color: {MUTED}; }
.time { font-size: 11px; letter-spacing: 0.2px; color: {MUTED}; font-feature-settings: 'tnum'; }
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

.notch scale { padding: 6px 0; min-width: 210px; }
.notch scale trough { min-height: 3px; border-radius: 99px; background: rgba(255,255,255,0.18); border: none; transition: min-height 140ms cubic-bezier(0.23, 1, 0.32, 1); }
.notch scale:hover trough { min-height: 5px; }
.notch scale highlight { background: {HIGHLIGHT}; border-radius: 99px; border: none; margin: 0; min-width: 0; }
.notch scale slider { min-width: 0; min-height: 0; margin: 0; border-radius: 99px; background: {HIGHLIGHT}; border: none; box-shadow: none; transition: min-width 140ms cubic-bezier(0.23, 1, 0.32, 1), min-height 140ms cubic-bezier(0.23, 1, 0.32, 1), margin 140ms cubic-bezier(0.23, 1, 0.32, 1); }
.notch scale:hover slider { min-width: 11px; min-height: 11px; margin: -3px; }

.notch button { background: none; border: none; box-shadow: none; color: #fff; min-width: 34px; min-height: 34px; border-radius: 6px; padding: 0; transition: transform 140ms cubic-bezier(0.23, 1, 0.32, 1), background 140ms ease, opacity 140ms ease; }
.notch button:hover { background: rgba(255,255,255,0.1); }
.notch button:active { transform: scale(0.96); }
.notch button.play { background: {HIGHLIGHT}; color: {GRAD2}; border-radius: 8px; }
.notch button.play:hover { background: alpha({HIGHLIGHT}, 0.84); }
.section { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: {MUTED}; }
.notch button.player-switch { font-size: 11px; min-height: 22px; padding: 0 8px; border-radius: 4px; background: rgba(255,255,255,0.08); }
.notch button.player-switch:hover { background: rgba(255,255,255,0.15); }
.shot { border-radius: 4px; transition: transform 140ms cubic-bezier(0.23, 1, 0.32, 1), opacity 140ms ease; }
.shot:hover { opacity: 0.92; }
.shot:active { transform: scale(0.97); }
.notch button.shot-remove { min-width: 20px; min-height: 20px; margin: 4px; border-radius: 4px; background: rgba(0,0,0,0.6); opacity: 0.45; }
.notch button.shot-remove:hover { background: rgba(0,0,0,0.85); opacity: 1; }
.no-media { font-size: 13px; color: {MUTED}; margin: 14px 24px; }
.page-nav { background: rgba(255,255,255,0.05); border: none; border-radius: 6px; padding: 2px; margin-bottom: 10px; }
.notch button.page-pill { min-width: 0; min-height: 22px; padding: 0 10px; border-radius: 4px; font-size: 11px; font-weight: 600; opacity: 0.54; }
.notch button.page-pill:hover { opacity: 0.9; background: rgba(255,255,255,0.06); }
.notch button.page-pill.active { opacity: 1; background: rgba(255,255,255,0.14); color: #fff; }

@keyframes pop { from { transform: scale(1); } 40% { transform: scale(1.035); } to { transform: scale(1); } }
.notch.pop { transform-origin: 50% 0; animation: pop 240ms cubic-bezier(0.23, 1, 0.32, 1); }
@keyframes fadein { from { opacity: 0; transform: translateY(2px); } to { opacity: 1; transform: translateY(0); } }
.compact.fade, .lyric.fade, .rom.fade { animation: fadein 200ms cubic-bezier(0.23, 1, 0.32, 1); }

.notch button.panel-toggle { min-width: 30px; min-height: 30px; border-radius: 6px; opacity: 0.58; }
.notch button.panel-toggle:checked { opacity: 1; background: rgba(255,255,255,0.14); }

.notch scrolledwindow, .notch list, .notch row { background: none; color: #fff; }
.notch scrollbar { opacity: 0; }
.notch row { padding: 0; border-radius: 0 6px 6px 0; }
.notch row:hover { background: rgba(255,255,255,0.06); }
.lrc { padding: 5px 10px; border-radius: 0 6px 6px 0; border-left: 2px solid transparent; color: #fff; opacity: 0.28; transition: opacity 160ms cubic-bezier(0.23, 1, 0.32, 1), background 160ms cubic-bezier(0.23, 1, 0.32, 1), border-color 160ms cubic-bezier(0.23, 1, 0.32, 1); }
.lrc .lrc-main { font-size: 12px; font-weight: 500; color: #fff; }
.lrc .lrc-rom { font-size: 10px; letter-spacing: 0.2px; color: {MUTED}; opacity: 0.75; }
.lrc.mid { opacity: 0.45; }
.lrc.near { opacity: 0.7; }
.lrc.near .lrc-main { font-size: 13px; }
.lrc.now { opacity: 1; background: rgba(255,255,255,0.06); border-left: 2px solid {ACCENT}; }
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

/* UI style: island (Dynamic Island / Soft Capsule) */
.notch.ui-island {
  border-radius: 0 0 26px 26px; padding: 6px 18px 8px;
  border: 1px solid rgba(255,255,255,0.14); border-top: none;
}
.notch.ui-island.open { border-radius: 0 0 32px 32px; padding: 8px 22px 18px; }
.notch.ui-island .mini-art { border-radius: 99px; }
.notch.ui-island .art { border-radius: 16px; border: 1px solid rgba(255,255,255,0.14); }
.notch.ui-island .eq > box { border-radius: 99px; }
.notch.ui-island .page-nav { border-radius: 99px; padding: 3px; background: rgba(255,255,255,0.08); }
.notch.ui-island button.page-pill { border-radius: 99px; padding: 0 12px; }
.notch.ui-island button { border-radius: 99px; }
.notch.ui-island button.play { border-radius: 99px; min-width: 42px; min-height: 42px; }
.notch.ui-island .timer-group { border-radius: 99px; padding: 2px 6px; background: rgba(255,255,255,0.08); }
.notch.ui-island button.timer-step, .notch.ui-island button.timer-run, .notch.ui-island .timer-badge, .notch.ui-island button.clip, .notch.ui-island button.player-switch { border-radius: 99px; }
.notch.ui-island .title.clock { font-size: 38px; font-weight: 800; letter-spacing: -1.6px; }
.notch.ui-island progressbar.stat-bar trough, .notch.ui-island progressbar.stat-bar progress, .notch.ui-island progressbar.dl trough, .notch.ui-island progressbar.dl progress { border-radius: 99px; min-height: 5px; }

/* UI style: hud (Tactical Monospace / Precision Grid) */
.notch.ui-hud {
  font-family: 'JetBrains Mono', 'Fira Code', monospace;
  border-radius: 0 0 4px 4px; padding: 5px 14px 7px;
  border: 1px solid alpha({ACCENT}, 0.35); border-top: none;
}
.notch.ui-hud.open { border-radius: 0 0 6px 6px; padding: 6px 16px 14px; }
.notch.ui-hud .compact { font-size: 12px; letter-spacing: 0.4px; }
.notch.ui-hud .mini-art, .notch.ui-hud .art, .notch.ui-hud .shot { border-radius: 2px; border: 1px solid rgba(255,255,255,0.2); }
.notch.ui-hud .title { font-size: 14px; letter-spacing: 0.2px; }
.notch.ui-hud .title.clock { font-size: 34px; font-weight: 700; letter-spacing: -0.5px; }
.notch.ui-hud .artist, .notch.ui-hud .time, .notch.ui-hud .stat-lbl { letter-spacing: 0.6px; }
.notch.ui-hud .page-nav { border-radius: 2px; background: rgba(255,255,255,0.04); border: 1px solid rgba(255,255,255,0.14); padding: 2px; }
.notch.ui-hud button.page-pill { border-radius: 2px; font-size: 10px; letter-spacing: 0.6px; }
.notch.ui-hud button.page-pill.active { background: alpha({ACCENT}, 0.22); color: {ACCENT}; }
.notch.ui-hud button { border-radius: 2px; border: 1px solid rgba(255,255,255,0.12); }
.notch.ui-hud button.play { border-radius: 2px; background: {ACCENT}; color: {GRAD1}; border-color: {ACCENT}; }
.notch.ui-hud scale trough, .notch.ui-hud scale highlight, .notch.ui-hud scale slider { border-radius: 0; }
.notch.ui-hud progressbar.stat-bar trough, .notch.ui-hud progressbar.stat-bar progress, .notch.ui-hud progressbar.dl trough, .notch.ui-hud progressbar.dl progress { border-radius: 0; min-height: 5px; }
.notch.ui-hud .timer-group { border-radius: 2px; border: 1px solid rgba(255,255,255,0.14); background: rgba(255,255,255,0.03); }
.notch.ui-hud button.timer-step, .notch.ui-hud button.timer-run, .notch.ui-hud .timer-badge, .notch.ui-hud button.clip { border-radius: 2px; }

/* UI style: studio (Modular Bento Cards & Side Dock) */
.notch.ui-studio {
  border-radius: 0 0 22px 22px; padding: 6px 16px 8px;
  border: 1px solid rgba(255,255,255,0.16); border-top: none;
}
.notch.ui-studio.open { border-radius: 0 0 24px 24px; padding: 12px 16px 14px; }
.notch.ui-studio .page-nav {
  background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1);
  border-radius: 14px; padding: 4px; margin-bottom: 0; margin-right: 12px;
}
.notch.ui-studio button.page-pill { border-radius: 10px; min-height: 26px; padding: 0 10px; }
.notch.ui-studio button.page-pill.active { background: {HIGHLIGHT}; color: {GRAD2}; font-weight: 700; }
.notch.ui-studio .clock-card {
  background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1);
  border-radius: 14px; padding: 10px 14px;
}
.notch.ui-studio .stats-card {
  background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.09);
  border-radius: 14px; padding: 10px 12px;
}
.notch.ui-studio .media-card {
  background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1);
  border-radius: 14px; padding: 10px 12px;
}
.notch.ui-studio .art { border-radius: 12px; border: 1px solid rgba(255,255,255,0.16); }
.notch.ui-studio .buttons-dock {
  background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1);
  border-radius: 14px; padding: 4px 10px;
}
.notch.ui-studio button { border-radius: 10px; }
.notch.ui-studio button.play { border-radius: 10px; min-width: 44px; min-height: 32px; }
.notch.ui-studio .timer-group {
  background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1);
  border-radius: 12px; padding: 3px 6px;
}
.notch.ui-studio .title.clock { font-size: 34px; font-weight: 800; letter-spacing: -1.2px; }
.notch.ui-studio progressbar.stat-bar trough, .notch.ui-studio progressbar.stat-bar progress { min-height: 6px; border-radius: 3px; }

/* Frosted glass material (Apple Design §12: translucent depth, diagonal sheen, specular rim, hierarchy & vibrancy) */
.notch.glass {
  backdrop-filter: blur(28px) saturate(190%) brightness(1.1);
  background:
    linear-gradient(145deg, rgba(255,255,255,0.26) 0%, rgba(255,255,255,0.09) 42%, rgba(255,255,255,0.03) 75%, rgba(255,255,255,0.10) 100%),
    linear-gradient(160deg, alpha({GRAD1}, 0.24) 0%, alpha({GRAD2}, 0.30) 65%, alpha({GRAD3}, 0.36) 100%);
  border: 1px solid rgba(255,255,255,0.34); border-top: none;
  box-shadow:
    inset 0 -1px 0 rgba(255,255,255,0.28),
    inset 1px 0 0 rgba(255,255,255,0.18),
    inset -1px 0 0 rgba(255,255,255,0.18);
}
.notch.glass.ui-hud {
  border-color: rgba(255,255,255,0.36);
  box-shadow:
    inset 0 -1px 0 alpha({ACCENT}, 0.45),
    inset 1px 0 0 rgba(255,255,255,0.22),
    inset -1px 0 0 rgba(255,255,255,0.22);
}
.notch.glass.thin { backdrop-filter: none; border: none; box-shadow: none; }
.notch.glass .compact, .notch.glass .title, .notch.glass .stat-val, .notch.glass .lyric {
  text-shadow: 0 1px 3px rgba(0,0,0,0.55);
}
.notch.glass .artist, .notch.glass .time, .notch.glass .section, .notch.glass .stat-lbl {
  color: rgba(255,255,255,0.88);
  text-shadow: 0 1px 2px rgba(0,0,0,0.45);
}
.notch.glass .page-nav, .notch.glass .timer-group, .notch.glass.ui-studio .clock-card, .notch.glass.ui-studio .stats-card, .notch.glass.ui-studio .media-card, .notch.glass.ui-studio .buttons-dock {
  background: linear-gradient(145deg, rgba(255,255,255,0.16) 0%, rgba(255,255,255,0.06) 100%);
  border: 1px solid rgba(255,255,255,0.24);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.22);
}
.notch.glass button.page-pill.active, .notch.glass button.panel-toggle:checked {
  background: rgba(255,255,255,0.28);
  color: #fff;
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.35);
}
.notch.glass button.clip, .notch.glass button.player-switch {
  background: rgba(255,255,255,0.14);
  border: 1px solid rgba(255,255,255,0.24);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.22);
}
.notch.glass button.clip:hover, .notch.glass button.player-switch:hover {
  background: rgba(255,255,255,0.24);
}
.notch.glass scale trough, .notch.glass progressbar.stat-bar trough, .notch.glass progressbar.dl trough {
  background: rgba(255,255,255,0.26);
}
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
