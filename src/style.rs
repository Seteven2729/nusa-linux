//! The notch stylesheet.

pub(crate) const CSS: &str = "
window { background: transparent; }
.notch {
  background: linear-gradient(160deg, #020510 0%, #061338 65%, #0c225c 100%); color: #fff;
  font-family: Inter, 'Noto Sans', sans-serif;
  border-radius: 0 0 20px 20px; padding: 5px 14px 7px; min-width: 150px;
  margin: 0 14px 14px; box-shadow: 0 6px 20px rgba(4,12,40,0.6);
  transition: border-radius 180ms ease, padding 180ms ease;
}
.notch.open { border-radius: 0 0 28px 28px; padding: 8px 16px 14px; }
.notch.thin { padding: 0; min-height: 5px; box-shadow: none; }
.compact { font-size: 13px; font-weight: 600; }
.mini-art { border-radius: 5px; }
.art { border-radius: 12px; }
.title { font-size: 15px; font-weight: 700; }
.title.clock { font-size: 30px; font-weight: 700; font-feature-settings: 'tnum'; }
.artist { font-size: 12px; color: #b9cdf5; }
.time { font-size: 11px; color: #b9cdf5; font-feature-settings: 'tnum'; }
.lyric { font-size: 13px; font-weight: 600; color: #fff; opacity: 0.9; margin-top: 2px; }

.eq { min-height: 14px; }
.eq > box { background: #8fd8ff; border-radius: 2px; min-width: 3px; min-height: 3px; }
progressbar.dl { margin-top: 3px; }
progressbar.dl trough { min-height: 3px; min-width: 0; border-radius: 2px; background: rgba(255,255,255,0.22); border: none; }
progressbar.dl progress { min-height: 3px; border-radius: 2px; background: #8fd8ff; border: none; }

.notch scale { padding: 0; min-width: 210px; }
.notch scale trough { min-height: 4px; border-radius: 2px; background: rgba(255,255,255,0.22); border: none; }
.notch scale highlight { background: #fff; border-radius: 2px; border: none; margin: 0; min-width: 0; }
.notch scale slider { min-width: 0; min-height: 0; margin: 0; background: #fff; border: none; box-shadow: none; transition: all 120ms ease; }
.notch scale:hover slider { min-width: 12px; min-height: 12px; margin: -4px; }

.notch button { background: none; border: none; box-shadow: none; color: #fff; min-width: 36px; min-height: 36px; border-radius: 999px; padding: 0; }
.notch button:hover { background: rgba(255,255,255,0.15); }
.notch button.play { background: #fff; color: #061338; }
.notch button.play:hover { background: #dce8ff; }
.notch button.panel-toggle { min-width: 30px; min-height: 30px; opacity: 0.6; }
.notch button.panel-toggle:checked { opacity: 1; background: rgba(255,255,255,0.15); }

.notch scrolledwindow, .notch list, .notch row { background: none; }
.notch row { padding: 0; border-radius: 6px; }
.notch row:hover { background: rgba(255,255,255,0.08); }
.lrc { font-size: 13px; padding: 3px 6px; opacity: 0.45; }
.lrc.now { opacity: 1; font-weight: 700; }
.rom { font-size: 11px; opacity: 0.6; }
.mic { color: #ff9f0a; font-size: 10px; }
.stats { font-size: 11px; color: #b9cdf5; font-feature-settings: 'tnum'; }
.notch button.clip { min-height: 26px; padding: 0 10px; border-radius: 8px; background: rgba(255,255,255,0.06); font-size: 12px; }
.notch button.clip:hover { background: rgba(255,255,255,0.14); }
";
