//! Google Input Tools web API, requested the way the extension does it (`sw`/`rw`/`Dw`).

use std::sync::OnceLock;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Query {
    pub itc: &'static str,
    pub text: String, // the `text` parameter before URL encoding
    pub num: usize,
}

/// JS `escape` of the characters the server treats as separators.
fn esc(s: &str, percent: bool) -> String {
    let s = if percent { s.replace('%', "%25") } else { s.to_string() };
    s.replace(',', "%2C").replace('|', "%7C").replace(':', "%3A")
}

impl Query {
    /// With pre-context the server sees `|<context>,<letters>`.
    pub fn new(itc: &'static str, ctx: &str, src: &str, num: usize) -> Self {
        let text = if ctx.is_empty() { esc(src, true) } else { format!("|{},{}", esc(ctx, false), esc(src, true)) };
        Query { itc, text, num }
    }

    pub fn url(&self) -> String {
        let enc: String = self
            .text
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect();
        format!(
            "https://inputtools.google.com/request?text={enc}&itc={}&num={}&cp=0&cs=1&ie=utf-8&oe=utf-8&app=chext",
            self.itc, self.num
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cands {
    pub words: Vec<String>,
    pub lens: Vec<usize>, // how many letters of the request each candidate consumes
    pub ann: Vec<String>, // romanization, shown grey by tools with config key 31
}

impl Cands {
    pub fn truncate(&mut self, n: usize) {
        self.words.truncate(n);
        self.lens.truncate(n);
        self.ann.truncate(n);
    }
}

/// POST with an empty body and everything in the query string, like the extension.
pub fn fetch(q: &Query, timeout: Duration) -> Option<Cands> {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    let agent = AGENT.get_or_init(|| ureq::AgentBuilder::new().build()); // keep-alive across lookups
    let body = agent.post(&q.url()).timeout(timeout).send_string("").ok()?.into_string().ok()?;
    let src_len = q.text.rsplit(',').next().unwrap_or("").len();
    parse(&body, src_len)
}

/// `src_len`: letters in the request, the default for candidates without matched_length.
pub fn parse(json: &str, src_len: usize) -> Option<Cands> {
    let json = &json[json.find('[')?..]; // the server may prefix `while(1);`
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get(0)?.as_str()? != "SUCCESS" {
        return None;
    }
    let item = v.get(1)?.get(0)?;
    let words: Vec<String> = item.get(1)?.as_array()?.iter().filter_map(|w| w.as_str().map(unescape)).collect();
    let meta = item.get(3);
    let list = |k: &str| meta.and_then(|m| m.get(k)).and_then(|m| m.as_array());
    let (ml, an) = (list("matched_length"), list("annotation"));
    let lens = (0..words.len()).map(|i| ml.and_then(|m| m.get(i)).and_then(|x| x.as_u64()).map_or(src_len, |x| x as usize)).collect();
    let ann = (0..words.len()).map(|i| an.and_then(|m| m.get(i)).and_then(|x| x.as_str()).unwrap_or("").to_string()).collect();
    Some(Cands { words, lens, ann })
}

/// `wn()`: the server HTML-escapes a few characters.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partial_matches_and_annotations() {
        let j = r#"["SUCCESS",[["ngodeigau",["我哋搞","我哋","我"],[],{"annotation":["ngo dei gaau","ngo dei","ngo"],"matched_length":[9,6,3]}]]]"#;
        let c = parse(j, 9).unwrap();
        assert_eq!(c.words, ["我哋搞", "我哋", "我"]);
        assert_eq!(&"ngodeigau"[c.lens[1]..], "gau");
        assert_eq!(c.ann[1], "ngo dei");
    }

    #[test]
    fn missing_metadata_defaults() {
        let c = parse(r#"while(1);["SUCCESS",[["nei",["你","&lt;3"],[],{}]]]"#, 3).unwrap();
        assert_eq!(c.lens, [3, 3]);
        assert_eq!(c.ann, ["", ""]);
        assert_eq!(c.words[1], "<3");
    }

    #[test]
    fn rejects_errors_and_garbage() {
        assert!(parse(r#"["INVALID_INPUT_METHOD_NAME"]"#, 1).is_none());
        assert!(parse("garbage", 1).is_none());
    }

    #[test]
    fn request_text_with_context() {
        let q = Query::new("yue-hant-t-i0-und", "我哋係,", "neihou", 13);
        assert_eq!(q.text, "|我哋係%2C,neihou");
        assert!(q.url().contains("text=%7C%E6%88%91%E5%93%8B%E4%BF%82%252C%2Cneihou&itc=yue-hant-t-i0-und&num=13"));
        assert_eq!(Query::new("x", "", "gam'jat", 13).text, "gam'jat");
    }
}
