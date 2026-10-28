//! Synced lyrics: lrclib fetch with disk cache, LRC parsing, romanization.

use crate::*;

/// Romanize every line with one script decision for the whole song, so a Japanese song's
/// kanji-only lines are read as Japanese, not Mandarin.
pub(crate) fn romanize_song(lyrics: &[(i64, String)]) -> Vec<Option<String>> {
    let has_kana = lyrics.iter().any(|(_, l)| l.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)));
    lyrics
        .iter()
        .map(|(_, l)| {
            let r = if l.chars().any(is_hangul) {
                romanize_korean(l)
            } else if has_kana {
                if !l.chars().any(|c| is_han(c) || ('\u{3040}'..='\u{30ff}').contains(&c)) {
                    return None;
                }
                romanize_japanese(l)
            } else if l.chars().any(is_han) {
                romanize_chinese(l)
            } else if l.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)) {
                romanize_cyrillic(l)
            } else {
                return None;
            };
            (r != *l).then_some(r)
        })
        .collect()
}

/// kakasi picks kanji readings from a dictionary without context, and gets a few of the
/// most common lyric words wrong (君 as "kun", 愛して as "itoshite", topic は as "ha").
/// Rewrite those to kana first.
/// ponytail: hand-picked fixes for frequent lyric words; a real morphological analyzer
/// (lindera + IPADIC) would fix readings generally but adds a ~50 MB dictionary.
pub(crate) fn romanize_japanese(s: &str) -> String {
    const FIXES: [(&str, &str); 9] = [
        ("君は", "きみわ"),
        ("僕は", "ぼくわ"),
        ("私は", "わたしわ"),
        ("俺は", "おれわ"),
        ("今日は", "きょうわ"),
        ("君", "きみ"),
        ("貴方", "あなた"),
        ("愛して", "あいして"),
        ("愛し", "あいし"),
    ];
    let mut text = s.to_string();
    for (from, to) in FIXES {
        text = text.replace(from, to);
    }
    kakasi::convert(text).romaji
}

pub(crate) fn is_hangul(c: char) -> bool {
    ('\u{ac00}'..='\u{d7a3}').contains(&c)
}

pub(crate) fn is_han(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c)
}

/// Revised Romanization, plus the two sound rules that matter most when singing along:
/// a final consonant carries over to a following silent ㅇ (있어 = isseo), and ㄹ before a vowel is r.
pub(crate) fn romanize_korean(s: &str) -> String {
    const INITIAL: [&str; 19] = ["g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h"];
    const VOWEL: [&str; 21] = [
        "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i",
    ];
    // Final consonant at the end of a syllable / when it moves onto a following vowel.
    const FINAL: [&str; 28] = ["", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "p", "l", "l", "p", "l", "m", "p", "p", "t", "t", "ng", "t", "t", "k", "t", "p", "t"];
    const LINKED: [&str; 28] = [
        "", "g", "kk", "ks", "n", "nj", "n", "d", "r", "lg", "lm", "lb", "ls", "lt", "lp", "r", "m", "b", "ps", "s", "ss", "ng", "j", "ch", "k", "t", "p", "",
    ];
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut carried = false; // this syllable's silent ㅇ already got the previous final
    for (i, &c) in chars.iter().enumerate() {
        if !is_hangul(c) {
            out.push(c);
            carried = false;
            continue;
        }
        let code = c as u32 - 0xac00;
        let (l, v, t) = ((code / 588) as usize, ((code % 588) / 28) as usize, (code % 28) as usize);
        if !(carried && l == 11) {
            out.push_str(INITIAL[l]);
        }
        out.push_str(VOWEL[v]);
        let next_silent = chars.get(i + 1).filter(|n| is_hangul(**n)).is_some_and(|n| (*n as u32 - 0xac00) / 588 == 11);
        carried = next_silent && t != 0;
        out.push_str(if carried { LINKED[t] } else { FINAL[t] });
    }
    out
}

pub(crate) fn romanize_chinese(s: &str) -> String {
    use pinyin::ToPinyin;
    let mut out = String::new();
    for (c, p) in s.chars().zip(s.to_pinyin()) {
        match p {
            Some(p) => {
                if !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push_str(p.with_tone());
            }
            None => out.push(c),
        }
    }
    out
}

pub(crate) fn romanize_cyrillic(s: &str) -> String {
    s.chars()
        .map(|c| {
            let lower = c.to_lowercase().next().unwrap_or(c);
            let r = match lower {
                'а' => "a", 'б' => "b", 'в' => "v", 'г' => "g", 'д' => "d", 'е' => "e", 'ё' => "yo", 'ж' => "zh",
                'з' => "z", 'и' => "i", 'й' => "y", 'к' => "k", 'л' => "l", 'м' => "m", 'н' => "n", 'о' => "o",
                'п' => "p", 'р' => "r", 'с' => "s", 'т' => "t", 'у' => "u", 'ф' => "f", 'х' => "kh", 'ц' => "ts",
                'ч' => "ch", 'ш' => "sh", 'щ' => "shch", 'ъ' | 'ь' => "", 'ы' => "y", 'э' => "e", 'ю' => "yu", 'я' => "ya",
                'і' => "i", 'ї' => "yi", 'є' => "ye", 'ґ' => "g",
                _ => return c.to_string(),
            };
            if c != lower {
                let mut cs = r.chars();
                cs.next().map(|f| f.to_uppercase().chain(cs).collect()).unwrap_or_default()
            } else {
                r.to_string()
            }
        })
        .collect()
}

/// Synced lyrics for a track: disk cache first, then lrclib's search and exact match,
/// preferring native script (Hangul/kana/Han/Cyrillic) over pre-romanized uploads.
pub(crate) fn fetch_lyrics(title: &str, artist: &str, duration_s: i64) -> Vec<(i64, String)> {
    let key: String = format!("{artist} - {title}").chars().map(|c| if c == '/' { '_' } else { c }).collect();
    let cache = glib::user_cache_dir().join("nusa/lyrics").join(format!("{key}.lrc"));
    if let Ok(lrc) = std::fs::read_to_string(&cache) {
        return parse_lrc(&lrc);
    }
    let get_json = |req: ureq::RequestBuilder<ureq::typestate::WithoutBody>| -> Option<serde_json::Value> {
        serde_json::from_str(&req.header("User-Agent", "nusa-linux (personal use)").call().ok()?.body_mut().read_to_string().ok()?).ok()
    };
    let exact = || {
        let mut req = ureq::get("https://lrclib.net/api/get")
            .query("track_name", title)
            .query("artist_name", artist);
        if duration_s > 0 {
            req = req.query("duration", duration_s.to_string());
        }
        get_json(req)?["syncedLyrics"].as_str().map(str::to_owned)
    };
    let searched = get_json(ureq::get("https://lrclib.net/api/search").query("q", format!("{artist} {title}")))
        .or_else(|| {
            get_json(
                ureq::get("https://lrclib.net/api/search")
                    .query("track_name", title)
                    .query("artist_name", artist),
            )
        })
        .and_then(|v| pick_best_lrc(&v, duration_s));
    let lrc = match (exact(), searched) {
        (Some(e), Some(s)) if !has_native_script(&e) && has_native_script(&s) => Some(s),
        (Some(e), _) => Some(e),
        (None, s) => s,
    };
    let Some(lrc) = lrc else { return vec![] };
    // Only successes are cached, so a song lrclib doesn't know yet is retried next time.
    let _ = std::fs::create_dir_all(cache.parent().unwrap()).and_then(|_| std::fs::write(&cache, &lrc));
    parse_lrc(&lrc)
}

fn has_native_script(s: &str) -> bool {
    s.chars()
        .any(|c| is_hangul(c) || is_han(c) || ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{0400}'..='\u{04ff}').contains(&c))
}

fn pick_best_lrc(results: &serde_json::Value, duration_s: i64) -> Option<String> {
    results
        .as_array()?
        .iter()
        .filter_map(|r| Some((r["syncedLyrics"].as_str()?, r["duration"].as_f64().unwrap_or(0.0) as i64)))
        .min_by_key(|(lrc, dur)| {
            let diff = if duration_s > 0 { (dur - duration_s).abs() } else { 0 };
            // Among tracks within 3 s of the target duration, prefer native script over romanized Latin.
            (diff > 3, !has_native_script(lrc), diff)
        })
        .map(|(lrc, _)| lrc.to_owned())
}

/// "[mm:ss.xx] text" lines into (ms, text), sorted.
pub(crate) fn parse_lrc(lrc: &str) -> Vec<(i64, String)> {
    let mut out: Vec<_> = lrc
        .lines()
        .filter_map(|l| {
            let (stamp, text) = l.strip_prefix('[')?.split_once(']')?;
            let (m, s) = stamp.split_once(':')?;
            let ms = m.parse::<i64>().ok()? * 60_000 + (s.parse::<f64>().ok()? * 1000.0) as i64;
            Some((ms, text.trim().to_string()))
        })
        .collect();
    out.sort_by_key(|(ms, _)| *ms);
    out
}

pub(crate) fn current_index(lyrics: &[(i64, String)], ms: i64) -> Option<usize> {
    lyrics.partition_point(|(t, _)| *t <= ms).checked_sub(1)
}

pub(crate) fn current_line(lyrics: &[(i64, String)], ms: i64) -> Option<String> {
    Some(lyrics.get(current_index(lyrics, ms)?)?.1.clone())
}

#[test]
pub(crate) fn lrc() {
    let l = parse_lrc("[00:12.50] second\n[ar:Someone]\n[00:01.00] first\n[01:00.00]");
    assert_eq!(l, vec![(1000, "first".into()), (12500, "second".into()), (60000, "".into())]);
    assert_eq!(current_line(&l, 500), None);
    assert_eq!(current_line(&l, 12500).as_deref(), Some("second"));
    assert_eq!(current_line(&l, 30000).as_deref(), Some("second"));
    assert_eq!(current_index(&l, 60000), Some(2));
    let candidates = serde_json::json!([
        {"duration": 225.0, "syncedLyrics": "[00:13.98]yeongwonhan geon jeoldae eopseo"},
        {"duration": 225.5, "syncedLyrics": "[00:13.98] 영원한 건 절대 없어"}
    ]);
    assert_eq!(pick_best_lrc(&candidates, 225).as_deref(), Some("[00:13.98] 영원한 건 절대 없어"));
}

#[test]
pub(crate) fn romanization() {
    assert_eq!(romanize_korean("사랑해"), "saranghae");
    assert_eq!(romanize_korean("있어"), "isseo");
    assert_eq!(romanize_korean("우리 사랑하지 말아요"), "uri saranghaji marayo");
    assert_eq!(romanize_korean("감사합니다"), "gamsahapnida");
    assert_eq!(romanize_chinese("我爱你"), "wǒ ài nǐ");
    assert_eq!(romanize_cyrillic("Привет, мир"), "Privet, mir");
    // A Japanese song: the kanji-only line is read as Japanese, not as Mandarin pinyin.
    let song = [(0, "君の名は".to_string()), (1, "名前".to_string())];
    let rom = romanize_song(&song);
    assert!(rom[0].as_deref().is_some_and(|r| r.starts_with("kimi")), "{rom:?}");
    assert!(rom[1].as_deref().is_some_and(|r| r.contains("namae")), "{rom:?}");
    assert!(romanize_japanese("愛してる").starts_with("aishi"));
    assert!(romanize_japanese("僕は君を忘れない").contains("bokuwa"));
    assert_eq!(romanize_song(&[(0, "hello".to_string())]), vec![None]);
}
