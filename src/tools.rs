//! The input tools Google Input Tools offers, with the per-tool settings from its
//! `imeconfigs/*.js` (config keys in brackets). Codes verified against the API 2026-10-07.

use crate::engine::{CHINESE, FULL, PUNCT};

pub struct Tool {
    pub itc: &'static str,
    pub name: &'static str,  // the registry's native name, which the extension shows
    pub label: &'static str, // the glyph on its sprite icon
    pub page: usize,         // [14] candidates per page
    pub vertical: bool,      // [11]
    pub annotate: bool,      // [31] grey romanization after each candidate
    pub chinese: bool,       // zh/yue family: state bits, status bar, Shift / Shift+Space / Ctrl+. hotkeys
    pub context: bool,       // [5] send the text before the caret
    pub extra: &'static str, // [22] composition characters besides a-z
    pub next_keys: &'static str, // [24]
    pub prev_keys: &'static str, // [25]
    pub radicals: bool,      // [21] cangjie shows 日月金... for a..z
    pub trad_quotes: bool,   // [19] override: ' -> 「」, " -> 『』
    pub rtl: bool,           // [28]
    pub digit0: Option<char>, // [1]/[17] numbering: None = no numbers, Some('0') = ASCII
    pub trailing_space: bool, // [3]
    pub enter_raw: bool,     // [4] Enter commits the letters, not the candidate
    pub caret_move: bool,    // [6] Left/Right/Home/End move inside the composition
    pub case_sensitive: bool, // [30]; false also means an uppercase first letter types raw
    pub keyboard: Option<&'static Keyboard>, // on-screen keyboard tool (`vkd_*`): no lookups
    pub accents: &'static [(&'static str, &'static str)], // [21] while composing: "`e" -> "è"
}

/// `fr_t_i0_und.js` table `f` (keys 19/21: the last two letters typed), generated from the file.
const FR_ACCENTS: &[(&str, &str)] = &[
    ("`A", "À"), ("`E", "È"), ("`I", "Ì"), ("`N", "Ǹ"), ("`O", "Ò"), ("`U", "Ù"), ("`W", "Ẁ"), ("`Y", "Ỳ"),
    ("`a", "à"), ("`e", "è"), ("`i", "ì"), ("`n", "ǹ"), ("`o", "ò"), ("`u", "ù"), ("`w", "ẁ"), ("`y", "ỳ"),
    ("^A", "Â"), ("^C", "Ĉ"), ("^E", "Ê"), ("^G", "Ĝ"), ("^H", "Ĥ"), ("^I", "Î"), ("^J", "Ĵ"), ("^O", "Ô"),
    ("^S", "Ŝ"), ("^U", "Û"), ("^W", "Ŵ"), ("^Y", "Ŷ"), ("^Z", "Ẑ"), ("^a", "â"), ("^c", "ĉ"), ("^e", "ê"),
    ("^g", "ĝ"), ("^h", "ĥ"), ("^i", "î"), ("^j", "ĵ"), ("^o", "ô"), ("^s", "ŝ"), ("^u", "û"), ("^w", "ŵ"),
    ("^y", "ŷ"), ("^z", "ẑ"), ("~A", "Ã"), ("~E", "Ẽ"), ("~I", "Ĩ"), ("~N", "Ñ"), ("~O", "Õ"), ("~U", "Ũ"),
    ("~V", "Ṽ"), ("~Y", "Ỹ"), ("~a", "ã"), ("~e", "ẽ"), ("~i", "ĩ"), ("~n", "ñ"), ("~o", "õ"), ("~u", "ũ"),
    ("~v", "ṽ"), ("~y", "ỹ"), ("\"A", "Ä"), ("\"E", "Ë"), ("\"H", "Ḧ"), ("\"I", "Ï"), ("\"O", "Ö"), ("\"U", "Ü"),
    ("\"W", "Ẅ"), ("\"X", "Ẍ"), ("\"Y", "Ÿ"), ("\"a", "ä"), ("\"e", "ë"), ("\"h", "ḧ"), ("\"i", "ï"), ("\"o", "ö"),
    ("\"t", "ẗ"), ("\"u", "ü"), ("\"w", "ẅ"), ("\"x", "ẍ"), ("\"y", "ÿ"), (">A", "Á"), (">C", "Ć"), (">E", "É"),
    (">G", "Ǵ"), (">I", "Í"), (">K", "Ḱ"), (">L", "Ĺ"), (">M", "Ḿ"), (">N", "Ń"), (">O", "Ó"), (">P", "Ṕ"),
    (">R", "Ŕ"), (">S", "Ś"), (">U", "Ú"), (">W", "Ẃ"), (">Y", "Ý"), (">Z", "Ź"), (">a", "á"), (">c", "ć"),
    (">e", "é"), (">g", "ǵ"), (">i", "í"), (">k", "ḱ"), (">l", "ĺ"), (">m", "ḿ"), (">n", "ń"), (">o", "ó"),
    (">p", "ṕ"), (">r", "ŕ"), (">s", "ś"), (">u", "ú"), (">w", "ẃ"), (">y", "ý"), (">z", "ź"),
];

/// ZC.transform / the imeconfig `g`: if `before + c` ends with a table key, how many
/// characters of `before` to replace and with what.
pub fn transform(table: &[(&str, &'static str)], before: &str, c: char) -> Option<(usize, &'static str)> {
    let s = format!("{before}{c}");
    table.iter().find(|(from, _)| s.ends_with(from)).map(|(from, to)| (from.chars().count() - 1, *to))
}

/// A `layouts/*.js` keyboard: its AltGr layers and `transform` table. The clickable
/// keyboard picture is not drawn, only the physical keys are remapped.
// ponytail: no base/Shift/Caps layers, so only layouts whose plain keys equal US
// QWERTY fit (us_intl); AZERTY (vkd_fr) needs them
pub struct Keyboard {
    pub altgr: &'static [(&'static str, &'static str)], // "c,cl": key codes -> chars
    pub altgr_shift: &'static [(&'static str, &'static str)], // "sc,scl"
    // ponytail: the extension's keys are regexes; these are literal (us_intl needs no more)
    pub transform: &'static [(&'static str, &'static str)],
}

/// `layouts/us_intl.js`, copied verbatim ("{{}}" = no character on that key).
pub const US_INTL: Keyboard = Keyboard {
    altgr: &[
        ("1234567890m=", "¡²³¤€¼½¾‘’¥×"),
        ("QWERTYUIOP", "äåé®þüúíóö"),
        ("\u{db}\u{dd}\u{dc}", "«»¬"),
        ("ASD", "áßđ"),
        (";\u{de}", "¶´"),
        ("ZXC", "æœ©"),
        ("NM", "ñµ"),
        ("\u{bc}", "ç"),
        ("\u{bf}", "¿"),
    ],
    altgr_shift: &[
        ("1", "¹"),
        ("4", "£"),
        ("=", "÷"),
        ("QWERTYUIOP", "ÄÅÉ{{}}ÞÜÚÍÓÖ"),
        ("\u{dc}", "¦"),
        ("ASD", "Á§Đ"),
        ("L;\u{de}", "Ø°¨"),
        ("ZXC", "ÆŒ¢"),
        ("N", "Ñ"),
        ("\u{bc}", "Ç"),
    ],
    transform: &[
        ("'a", "á"), ("'e", "é"), ("'i", "í"), ("'o", "ó"), ("'u", "ú"), ("'c", "ç"),
        ("'A", "Á"), ("'E", "É"), ("'I", "Í"), ("'O", "Ó"), ("'U", "Ú"), ("'C", "Ç"),
        ("`a", "à"), ("`e", "è"), ("`i", "ì"), ("`o", "ò"), ("`u", "ù"),
        ("`A", "À"), ("`E", "È"), ("`I", "Ì"), ("`O", "Ò"), ("`U", "Ù"),
        ("\"a", "ä"), ("\"e", "ë"), ("\"i", "ï"), ("\"o", "ö"), ("\"u", "ü"),
        ("\"A", "Ä"), ("\"E", "Ë"), ("\"I", "Ï"), ("\"O", "Ö"), ("\"U", "Ü"),
        ("^a", "â"), ("^e", "ê"), ("^i", "î"), ("^o", "ô"), ("^u", "û"),
        ("^A", "Â"), ("^E", "Ê"), ("^I", "Î"), ("^O", "Ô"), ("^U", "Û"),
        ("~a", "ã"), ("~o", "õ"), ("~n", "ñ"), ("~A", "Ã"), ("~O", "Õ"), ("~N", "Ñ"),
    ],
};

impl Keyboard {
    /// What AltGr (Ctrl+Alt) + this Windows virtual key types, if the layout has it.
    pub fn altgr(&self, vk: u32, shift: bool) -> Option<char> {
        // the layouts use Firefox key codes for - = ;
        let code = |k: char| match k {
            'm' => 0xBD,
            '=' => 0xBB,
            ';' => 0xBA,
            _ => k as u32,
        };
        let rows = if shift { self.altgr_shift } else { self.altgr };
        rows.iter().find_map(|(keys, chars)| {
            let i = keys.chars().position(|k| code(k) == vk)?;
            chars.replace("{{}}", "\0").chars().nth(i).filter(|&c| c != '\0')
        })
    }

    /// Characters before the caret to delete and what to type instead, if any.
    pub fn transform(&self, before: &str, c: char) -> Option<(usize, &'static str)> {
        transform(self.transform, before, c)
    }
}

const fn zh(itc: &'static str, name: &'static str, label: &'static str) -> Tool {
    Tool {
        itc,
        name,
        label,
        page: 5,
        vertical: false,
        annotate: false,
        chinese: true,
        context: true,
        extra: "'",
        next_keys: "=.",
        prev_keys: "-,",
        radicals: false,
        trad_quotes: false,
        rtl: false,
        digit0: Some('0'),
        trailing_space: false,
        enter_raw: true,
        caret_move: true,
        case_sensitive: false,
        keyboard: None,
        accents: &[],
    }
}

const fn yue(itc: &'static str, name: &'static str) -> Tool {
    Tool { page: 6, vertical: true, annotate: true, trad_quotes: true, ..zh(itc, name, "粵") }
}

const fn cangjie(itc: &'static str, name: &'static str) -> Tool {
    Tool { context: false, annotate: true, extra: "'.*", next_keys: "=", radicals: true, ..zh(itc, name, "倉") }
}

const fn shuangpin(itc: &'static str, name: &'static str, extra: &'static str) -> Tool {
    Tool { extra, ..zh(itc, name, "双") }
}

const fn tr(itc: &'static str, name: &'static str, label: &'static str) -> Tool {
    Tool {
        itc,
        name,
        label,
        page: 6,
        vertical: true,
        annotate: false,
        chinese: false,
        context: false,
        extra: "",
        next_keys: "",
        prev_keys: "",
        radicals: false,
        trad_quotes: false,
        rtl: false,
        digit0: Some('0'),
        trailing_space: true,
        enter_raw: false,
        caret_move: false,
        case_sensitive: true,
        keyboard: None,
        accents: &[],
    }
}

const fn kbd(itc: &'static str, name: &'static str, label: &'static str, k: &'static Keyboard) -> Tool {
    Tool { keyboard: Some(k), trailing_space: false, ..tr(itc, name, label) }
}

const fn ctx(t: Tool) -> Tool {
    Tool { context: true, ..t }
}

const fn rtl(t: Tool) -> Tool {
    Tool { rtl: true, ..t }
}

/// hi/kn/mr/ne: digits and ^~| are composition characters (key 22).
const INDIC: &str = "0123456789^~|";

/// Same order as the extension's registry.
pub const TOOLS: &[Tool] = &[
    Tool { extra: "`", ..tr("am-t-i0-und", "አማርኛ", "አ") },
    // ar: digits and Space are composition letters too
    Tool { digit0: None, extra: "0123456789`_-' ", ..rtl(tr("ar-t-i0-und", "العربية", "ع")) },
    Tool { digit0: Some('\u{09E6}'), ..tr("bn-t-i0-und", "বাংলা", "বা") },
    zh("zh-t-i0-pinyin", "拼音", "拼"),
    shuangpin("zh-t-i0-pinyin-x0-shuangpin-abc", "双拼（智能ABC）", "'"),
    shuangpin("zh-t-i0-pinyin-x0-shuangpin-ms", "双拼（微软方案）", "';"),
    shuangpin("zh-t-i0-pinyin-x0-shuangpin-flypy", "双拼（小鹤）", "'"),
    shuangpin("zh-t-i0-pinyin-x0-shuangpin-jiajia", "双拼（拼音加加）", "'"),
    shuangpin("zh-t-i0-pinyin-x0-shuangpin-ziguang", "双拼（紫光）", "';"),
    shuangpin("zh-t-i0-pinyin-x0-shuangpin-ziranma", "双拼（自然码）", "'"),
    Tool { extra: "`", ..tr("el-t-i0-und", "Ελληνικά", "ελ") },
    tr("gu-t-i0-und", "ગુજરાતી", "ગુ"),
    Tool { extra: INDIC, ..ctx(tr("hi-t-i0-und", "हिन्दी", "अ")) },
    Tool { extra: "`", ..ctx(rtl(tr("he-t-i0-und", "עִבְרִית", "א"))) },
    Tool { digit0: Some('\u{0CE6}'), extra: INDIC, ..tr("kn-t-i0-und", "ಕನ್ನಡ", "ಕ") },
    tr("ml-t-i0-und", "മലയാളം (സാധാരണ ലിപിമാറ്റം)", "മ"),
    Tool { digit0: Some('\u{0966}'), extra: INDIC, ..tr("mr-t-i0-und", "मराठी", "म") },
    Tool { extra: INDIC, ..tr("ne-t-i0-und", "नेपाली", "ने") },
    tr("or-t-i0-und", "ଓଡ଼ିଆ", "ଓ"),
    Tool { digit0: Some('\u{06F0}'), extra: "`'", ..rtl(tr("fa-t-i0-und", "فارسی", "ف")) },
    tr("pa-t-i0-und", "ਪੰਜਾਬੀ", "ਅ"),
    Tool { extra: "'[]\\`", ..ctx(tr("ru-t-i0-und", "Русский", "Py")) },
    tr("sa-t-i0-und", "संस्कृतम्", "सं"),
    Tool { extra: "ĆćČčĐđŠšŽž", ..tr("sr-t-i0-und", "Српски", "Cp") },
    tr("si-t-i0-und", "සිංහල", "සි"),
    Tool { extra: "^_", ..tr("ta-t-i0-und", "தமிழ்", "த") },
    Tool { extra: "^~|@", ..tr("te-t-i0-und", "తెలుగు", "అ") },
    Tool { extra: "`", ..tr("ti-t-i0-und", "ትግርኛ", "ት") },
    Tool { digit0: None, extra: "0123456789", ..rtl(tr("ur-t-i0-und", "اردو", "ا")) },
    Tool { page: 6, context: false, annotate: true, ..zh("zh-t-i0-wubi-1986", "五笔", "五") },
    // 注音 (zh-hant-t-i0-und) needs the extension's separate bopomofo model; not ported
    Tool { trad_quotes: true, ..zh("zh-hant-t-i0-pinyin", "漢語拼音", "拼") },
    Tool { page: 8, ..ctx(tr("vi-t-i0-und", "Tiếng Việt", "ê")) },
    Tool { extra: "'[]\\", ..ctx(tr("be-t-i0-und", "Беларуская", "Бе")) },
    Tool { extra: "'[]\\`", ..tr("bg-t-i0-und", "Български", "Бъ") },
    Tool { extra: "'[]\\", ..ctx(tr("uk-t-i0-und", "Транслітерація", "Yk")) },
    cangjie("zh-hant-t-i0-cangjie-1982", "倉頡"),
    cangjie("zh-hant-t-i0-cangjie-1987", "倉頡（五代）"),
    cangjie("zh-hant-t-i0-cangjie-1987-x-m0-simplified", "速成"),
    yue("yue-hant-t-i0-und", "廣東話"),
    yue("yue-hant-t-i0-jyutping", "粵拼"),
    Tool { trailing_space: false, ..ctx(tr("th-t-i0-und", "ภาษาไทย", "ก")) },
    // vkd_us_intl: one layout under seven languages; the extension's other keyboards are not ported
    kbd("nl-t-k0-intl", "Nederlands - US International", "NL", &US_INTL),
    kbd("fr-t-k0-intl", "Français - US International", "FR", &US_INTL),
    kbd("de-t-k0-intl", "Deutsch - US International", "DE", &US_INTL),
    kbd("it-t-k0-intl", "Italiano - US International", "IT", &US_INTL),
    kbd("pt-br-t-k0-intl", "Português (Brasil) - US International", "BR", &US_INTL),
    kbd("pt-pt-t-k0-intl", "Português (Portugal) - US International", "PT", &US_INTL),
    kbd("es-t-k0-intl", "Español - US International", "ES", &US_INTL),
    // in the registry but hidden by the extension; kept because the API serves them
    yue("yue-hant-t-i0-yale", "廣東話（耶魯）"),
    Tool { extra: "`~\"^>'-", accents: FR_ACCENTS, ..ctx(tr("fr-t-i0-und", "Français", "Fr")) },
    // not offered by the extension (its ja config is hidden); kept because the API serves it
    Tool { page: 9, trailing_space: false, case_sensitive: false, ..tr("ja-t-i0-und", "日本語", "あ") },
];

pub fn find(itc: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.itc == itc)
}

/// Font for candidate words. Arial (Google's box font) has no CJK fallback in GDI.
pub fn font_for(t: &Tool) -> &'static str {
    if t.itc.starts_with("ja") {
        "Yu Gothic UI"
    } else if t.itc.starts_with("zh-t") {
        "Microsoft YaHei"
    } else if t.chinese {
        "Microsoft JhengHei"
    } else {
        "Segoe UI" // GDI falls back per script for Indic, Arabic, Ethiopic...
    }
}

pub fn number(t: &Tool, n: usize) -> String {
    match t.digit0 {
        None => String::new(),
        Some('0') => format!("{n}. "),
        Some(z) => {
            let digits: String = n.to_string().chars().map(|d| char::from_u32(z as u32 + d as u32 - '0' as u32).unwrap()).collect();
            format!("{digits}. ")
        }
    }
}

pub fn radical(c: char) -> Option<char> {
    let i = (c as u32).checked_sub('a' as u32)? as usize;
    "日月金木水火土竹戈十大中一弓人心手口尸廿山女田難卜重".chars().nth(i)
}

/// Full-width form (table `l` in the Chinese configs: letters, digits and these symbols).
fn full_width(c: char) -> Option<char> {
    (c.is_ascii_alphanumeric() || "~!@#$^&*()-_[]{}\\|;:'\",.<>/?".contains(c)).then(|| char::from_u32(c as u32 + 0xFEE0).unwrap())
}

/// Key 19: what `c` turns into when typed outside (or to end) a composition, if anything.
pub fn convert(t: &Tool, state: u8, c: char, quotes: &mut [bool; 2]) -> Option<String> {
    if !t.chinese {
        let lang = &t.itc[..2];
        // native digits: bn/kn/mr/fa own block, ur Arabic-Indic
        let zero = match lang {
            "bn" => Some('\u{09E6}'),
            "kn" => Some('\u{0CE6}'),
            "mr" => Some('\u{0966}'),
            "fa" => Some('\u{06F0}'),
            "ur" => Some('\u{0660}'),
            _ => None,
        };
        if let (Some(z), Some(d)) = (zero, c.to_digit(10)) {
            return char::from_u32(z as u32 + d).map(String::from);
        }
        let s = match (lang, c) {
            ("am" | "ti", '.') => "።",
            ("am" | "ti", ',') => "፣",
            ("ar" | "ur", ',') => "،",
            ("ar" | "ur" | "fa", ';') => "؛",
            ("ar" | "ur" | "fa", '?') => "؟",
            ("bn" | "kn" | "mr" | "gu" | "ne", '|') => "।",
            _ => return None, // ponytail: the Indic danda after a long word is not ported
        };
        return Some(s.to_string());
    }
    let zh_mode = state & CHINESE != 0;
    if state & PUNCT != 0 && zh_mode && !(t.radicals && c == '*') {
        let mut pair = |i: usize, p: [&str; 2]| {
            let s = p[quotes[i] as usize];
            quotes[i] = !quotes[i];
            Some(s.to_string())
        };
        let s = match c {
            '\'' => return pair(0, if t.trad_quotes { ["「", "」"] } else { ["‘", "’"] }),
            '"' => return pair(1, if t.trad_quotes { ["『", "』"] } else { ["“", "”"] }),
            '~' => "～",
            '!' => "！",
            '$' => "￥",
            '^' => "……",
            '*' => "×",
            '(' => "（",
            ')' => "）",
            '-' => "－",
            '_' => "——",
            '[' => "【",
            ']' => "】",
            '{' => "｛",
            '}' => "｝",
            '\\' => "、",
            ';' => "；",
            ':' => "：",
            ',' => "，",
            '.' => "。",
            '<' => "《",
            '>' => "》",
            '/' => "／",
            '?' => "？",
            _ => "",
        };
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    if state & FULL != 0 && !(zh_mode && c.is_ascii_alphabetic()) {
        return full_width(c).map(String::from);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_unique_and_42_google_tools() {
        let mut codes: Vec<_> = TOOLS.iter().map(|t| t.itc).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), TOOLS.len());
        assert_eq!(TOOLS.len(), 41 + 7 + 3); // 42 offered - 注音 + 7 US International + Yale, French, 日本語
    }

    #[test]
    fn us_intl_altgr_and_transforms() {
        let k = &US_INTL;
        assert_eq!(k.altgr('E' as u32, false), Some('é'));
        assert_eq!(k.altgr(0xBD, false), Some('¥'), "minus key");
        assert_eq!(k.altgr(0xBC, true), Some('Ç'), "comma key");
        assert_eq!(k.altgr('R' as u32, true), None, "{{}} placeholder");
        assert_eq!(k.altgr('T' as u32, true), Some('Þ'), "after the placeholder");
        assert_eq!(k.altgr('G' as u32, false), None);
        assert_eq!(k.transform("l'", 'e'), Some((1, "é")));
        assert_eq!(k.transform("", '^'), None);
        assert_eq!(k.transform("x", 'e'), None);
    }

    #[test]
    fn chinese_punctuation() {
        let yue = find("yue-hant-t-i0-und").unwrap();
        let py = find("zh-t-i0-pinyin").unwrap();
        let cj = find("zh-hant-t-i0-cangjie-1987").unwrap();
        let mut q = [false; 2];
        let on = CHINESE | PUNCT;
        assert_eq!(convert(yue, on, ',', &mut q).as_deref(), Some("，"));
        assert_eq!(convert(py, on, '\\', &mut q).as_deref(), Some("、"));
        let quotes: Vec<_> = (0..3).map(|_| convert(yue, on, '\'', &mut q).unwrap()).collect();
        assert_eq!(quotes, ["「", "」", "「"]);
        let mut q = [false; 2];
        assert_eq!(convert(py, on, '"', &mut q).as_deref(), Some("“"));
        assert_eq!(convert(cj, on, '\'', &mut q).as_deref(), Some("‘"));
        assert_eq!(convert(cj, on, '*', &mut q), None);
        assert_eq!(convert(yue, on, '@', &mut q), None);
        assert_eq!(convert(yue, CHINESE, ',', &mut q), None); // punctuation off
        assert_eq!(convert(find("hi-t-i0-und").unwrap(), on, ',', &mut q), None);
        assert_eq!(convert(find("ar-t-i0-und").unwrap(), 1, '?', &mut q).as_deref(), Some("؟"));
    }

    #[test]
    fn full_width_mode() {
        let yue = find("yue-hant-t-i0-und").unwrap();
        let mut q = [false; 2];
        assert_eq!(convert(yue, CHINESE | FULL, '7', &mut q).as_deref(), Some("７"));
        assert_eq!(convert(yue, CHINESE | FULL, 'a', &mut q), None); // letters compose in Chinese mode
        assert_eq!(convert(yue, FULL, 'a', &mut q).as_deref(), Some("ａ"));
        assert_eq!(convert(yue, FULL, ',', &mut q).as_deref(), Some("，"));
        assert_eq!(convert(yue, 0, 'a', &mut q), None);
    }

    #[test]
    fn numbering_and_radicals() {
        assert_eq!(number(find("yue-hant-t-i0-und").unwrap(), 3), "3. ");
        assert_eq!(number(find("bn-t-i0-und").unwrap(), 3), "৩. ");
        assert_eq!(number(find("ar-t-i0-und").unwrap(), 3), "");
        assert_eq!(radical('a'), Some('日'));
        assert_eq!(radical('z'), Some('重'));
        assert_eq!(radical('*'), None);
    }
}
