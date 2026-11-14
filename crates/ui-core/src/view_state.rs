//! Name a widget with [`crate::UiTree::set_autosave_name`]. [`crate::UiTree::view_state`]
//! collects named widgets' [`crate::Widget::save_state`]; after
//! [`crate::UiTree::restore_view_state`] the tree hands it back through
//! [`crate::Widget::restore_state`], also to widgets named later.

use std::collections::BTreeMap;
use std::fmt::Write as _;

const HEADER: &str = "# noa view state 1";

/// Saved view states, keyed by autosave name, in name order.
///
/// The text form has one `name = value` line per entry. Backslashes and line breaks are
/// escaped in both (`\\`, `\n`, `\r`), and `=` and a leading `#` in names (`\=`,
/// `\#`). Blank lines and lines starting with an unescaped `#` are comments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewState {
    entries: BTreeMap<String, String>,
}

impl ViewState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(String::as_str)
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.entries.insert(name.into(), value.into());
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.entries.remove(name)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// In name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Add `other`'s entries, replacing those with the same names.
    pub fn merge(&mut self, other: ViewState) {
        self.entries.extend(other.entries);
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from(HEADER);
        out.push('\n');
        for (name, value) in &self.entries {
            escape(&mut out, name, true);
            out.push_str(" = ");
            escape(&mut out, value, false);
            out.push('\n');
        }
        out
    }

    /// Never fails: a file written by one version of an application is read by the next,
    /// so lines that are not `name = value` are skipped.
    pub fn parse(text: &str) -> Self {
        let mut entries = BTreeMap::new();
        for line in text.lines() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let Some(eq) = unescaped_eq(line) else {
                continue;
            };
            let name = &line[..eq];
            let name = name.strip_suffix(' ').unwrap_or(name);
            let value = &line[eq + 1..];
            let value = value.strip_prefix(' ').unwrap_or(value);
            if name.is_empty() {
                continue;
            }
            entries.insert(unescape(name), unescape(value));
        }
        Self { entries }
    }
}

impl FromIterator<(String, String)> for ViewState {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        Self {
            entries: iter.into_iter().collect(),
        }
    }
}

fn escape(out: &mut String, s: &str, name: bool) {
    if name && s.starts_with('#') {
        out.push('\\');
    }
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '=' if name => out.push_str("\\="),
            c => out.push(c),
        }
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(c) => out.push(c),
            None => out.push('\\'),
        }
    }
    out
}

/// The byte offset of the first `=` not escaped by a backslash.
fn unescaped_eq(line: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '=' => return Some(i),
            _ => {}
        }
    }
    None
}

/// The fields of a widget's saved value: `key:value` parts separated by `;`. Parts
/// without a `:` have an empty value.
pub(crate) fn fields(value: &str) -> impl Iterator<Item = (&str, &str)> {
    value.split(';').filter(|f| !f.trim().is_empty()).map(|f| {
        let (k, v) = f.split_once(':').unwrap_or((f, ""));
        (k.trim(), v.trim())
    })
}

pub(crate) fn field<'a>(value: &'a str, key: &str) -> Option<&'a str> {
    fields(value).find(|(k, _)| *k == key).map(|(_, v)| v)
}

/// A length to save: rounded to a hundredth of a pixel, without trailing zeros.
pub(crate) fn push_number(out: &mut String, n: f32) {
    let n = (n * 100.0).round() / 100.0;
    let _ = write!(out, "{n}");
}

/// Accepts only finite, non-negative lengths.
pub(crate) fn parse_length(s: &str) -> Option<f32> {
    s.trim()
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
}

/// Comma-separated numbers (any finite value), all or nothing.
pub(crate) fn parse_numbers(s: &str) -> Option<Vec<f32>> {
    s.split(',')
        .map(|n| n.trim().parse::<f32>().ok().filter(|n| n.is_finite()))
        .collect()
}

/// Item paths (titles from the top down) as one line: titles joined by `/`, with `/`
/// and `\` in titles escaped by a backslash.
pub(crate) fn join_path<'a>(titles: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for (i, title) in titles.into_iter().enumerate() {
        if i > 0 {
            out.push('/');
        }
        for c in title.chars() {
            if matches!(c, '/' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips_awkward_names_and_values() {
        let mut state = ViewState::new();
        state.set("finder.split", "sizes:240,-;collapsed:");
        state.set("a = b\\c", "line 1\nline 2\r\\ end ");
        state.set("trailing ", " leading");
        let text = state.to_text();
        assert!(text.starts_with(HEADER));
        assert_eq!(ViewState::parse(&text), state);
    }

    #[test]
    fn names_starting_with_a_hash_are_not_saved_as_comments() {
        let mut state = ViewState::new();
        for name in ["#panel", "#", "##two", "#a = b\\c", "plain#name"] {
            state.set(name, "#value\nwith a second line");
        }
        assert_eq!(ViewState::parse(&state.to_text()), state);
        let read = ViewState::parse("#comment = ignored\n\\#panel = restored\n");
        assert_eq!(read.get("#panel"), Some("restored"));
        assert_eq!(read.len(), 1);
        let ordinary = ViewState::from_iter([("plain#name".into(), "#value".into())]);
        assert_eq!(
            ordinary.to_text(),
            "# noa view state 1\nplain#name = #value\n"
        );
    }

    #[test]
    fn parse_skips_what_it_cannot_read() {
        let state = ViewState::parse("# comment\n\nnot a pair\n = no name\nok = 1\r\nx=2");
        let entries: Vec<_> = state.iter().collect();
        assert_eq!(entries, [("ok", "1"), ("x", "2")]);
    }

    #[test]
    fn fields_and_numbers() {
        assert_eq!(field("columns:3; sort:1,asc", "sort"), Some("1,asc"));
        assert_eq!(field("expanded", "expanded"), Some(""));
        assert_eq!(field("columns:3", "sort"), None);
        assert_eq!(parse_numbers("1, 2.5,-3"), Some(vec![1.0, 2.5, -3.0]));
        assert_eq!(parse_numbers("1,x"), None);
        assert_eq!(parse_length("-1"), None);
        assert_eq!(parse_length("NaN"), None);
        let mut s = String::new();
        push_number(&mut s, 120.0);
        s.push(',');
        push_number(&mut s, 33.333_33);
        assert_eq!(s, "120,33.33");
    }

    #[test]
    fn paths_escape_slashes() {
        let path = join_path(["Places", "a/b", "c\\d"]);
        assert_eq!(path, "Places/a\\/b/c\\\\d");
    }
}
