//! Kana helpers for Japanese forms: telling kana from other text, converting between
//! hiragana and katakana, and normalizing character widths.
//!
//! A furigana field usually takes the reading a [`crate::widgets::TextField`] captured
//! from the IME (see [`crate::widgets::ReadingChanged`]) and stores it in one script:
//! `to_katakana(reading)` for フリガナ, `to_hiragana(reading)` for ふりがな.

/// Whether `text` is not empty and made only of kana: hiragana, katakana, the
/// prolonged sound mark `ー`, the middle dot `・` and the iteration marks.
pub fn is_kana(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_kana_char)
}

fn is_kana_char(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{3096}' | '\u{309D}'..='\u{309F}' | '\u{30A1}'..='\u{30FF}')
}

/// Hiragana become katakana (`やまだ` → `ヤマダ`); everything else is kept.
pub fn to_katakana(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{3041}'..='\u{3096}' | '\u{309D}' | '\u{309E}' => shift(c, 0x60),
            _ => c,
        })
        .collect()
}

/// Katakana that have a hiragana form become hiragana (`ヤマダ` → `やまだ`); everything
/// else, including `ー` and `・`, is kept.
pub fn to_hiragana(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{30A1}'..='\u{30F6}' | '\u{30FD}' | '\u{30FE}' => shift(c, -0x60),
            _ => c,
        })
        .collect()
}

fn shift(c: char, by: i32) -> char {
    char::from_u32((c as i32 + by) as u32).unwrap_or(c)
}

/// The usual width normalization for form input: full-width ASCII letters, digits and
/// symbols become half-width (`ＡＢＣ１２３` → `ABC123`), and half-width katakana become
/// full-width, with a following `ﾞ` / `ﾟ` merged into the letter (`ｶﾞｯｺｳ` → `ガッコウ`,
/// and `カﾞ` → `ガ`, so normalizing after every keystroke gives the same result).
/// The full-width space `　` is kept, since Japanese names put it between family and
/// given name.
pub fn normalize_width(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{FF01}'..='\u{FF5E}' => out.push(shift(c, -0xFEE0)),
            '\u{FF61}'..='\u{FF9F}' => {
                let full = HALF_WIDTH_KANA[(c as usize) - 0xFF61];
                // A mark merges into the letter before it, whether that came half-width
                // in the same text or was normalized earlier (marks typed one at a time).
                let merged = match full {
                    '゛' => out.chars().next_back().and_then(voiced),
                    '゜' => out.chars().next_back().and_then(semi_voiced),
                    _ => None,
                };
                if let Some(merged) = merged {
                    out.pop();
                    out.push(merged);
                } else {
                    out.push(full);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// `ｶ` + `ﾞ` → `ガ`, `ｳ` + `ﾞ` → `ヴ`.
fn voiced(base: char) -> Option<char> {
    match base {
        'ウ' => Some('ヴ'),
        'カ' | 'キ' | 'ク' | 'ケ' | 'コ' | 'サ' | 'シ' | 'ス' | 'セ' | 'ソ' | 'タ' | 'チ'
        | 'ツ' | 'テ' | 'ト' | 'ハ' | 'ヒ' | 'フ' | 'ヘ' | 'ホ' => Some(shift(base, 1)),
        _ => None,
    }
}

/// `ﾊ` + `ﾟ` → `パ`.
fn semi_voiced(base: char) -> Option<char> {
    matches!(base, 'ハ' | 'ヒ' | 'フ' | 'ヘ' | 'ホ').then(|| shift(base, 2))
}

/// U+FF61 ..= U+FF9F in order.
const HALF_WIDTH_KANA: [char; 63] = [
    '。', '「', '」', '、', '・', 'ヲ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ャ', 'ュ', 'ョ', 'ッ', 'ー',
    'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ',
    'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ', 'マ', 'ミ',
    'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ', 'ン', '゛', '゜',
];
