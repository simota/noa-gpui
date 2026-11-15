//! What an input method may ask about the focused text, beyond where the caret is:
//! the selection to convert again, the text around it, and where each character is
//! (to place its candidate window and to find the character under the pointer).

use std::ops::Range;

use noa_gpui_scene::{Point, Rect, TextLayoutId};
use noa_gpui_text::TextStore;

/// Bytes on either side of the caret the input method sees at most: a long paragraph
/// is cut there rather than copied whole every frame.
const REACH: usize = 2000;

/// The text around the caret as shown, with a composition in place of the selection,
/// and the geometry of its characters. Ranges are bytes of [`ImeContext::text`];
/// rectangles are in the widget's coordinates until [`crate::UiTree::ime_context`]
/// puts them in the window's.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImeContext {
    /// The paragraph holding the caret (or as much of it as [`REACH`] allows).
    pub text: String,
    /// Where [`ImeContext::text`] starts in the text shown, in bytes. An
    /// [`crate::Event::ImeSelect`] range is relative to it too.
    pub base: usize,
    /// The selection, or the caret; inside the composition while there is one.
    pub selection: Range<usize>,
    /// The composition, while the input method has one.
    pub composition: Option<Range<usize>>,
    /// Every character's cluster and its box, in text order.
    pub clusters: Vec<(Range<usize>, Rect)>,
    /// The caret's box, for ranges with no characters (an empty text).
    pub caret: Rect,
}

impl ImeContext {
    /// The context of `layout` for a widget that draws it at `origin`: the paragraph
    /// around `selection`, with `composition` if there is one. `shown` is the part of
    /// the text laid out that starts at byte `shown_base` and holds that paragraph (at
    /// least); the ranges are bytes of the whole text.
    pub fn of_layout(
        text: &TextStore,
        layout: TextLayoutId,
        (shown, shown_base): (&str, usize),
        selection: Range<usize>,
        composition: Option<Range<usize>>,
        origin: Point,
    ) -> Self {
        let into_shown = |r: &Range<usize>| {
            r.start.saturating_sub(shown_base).min(shown.len())
                ..r.end.saturating_sub(shown_base).min(shown.len())
        };
        let selection = into_shown(&selection);
        let composition = composition.as_ref().map(into_shown);
        let at = selection.start;
        let mut start = shown[..at].rfind('\n').map_or(0, |i| i + 1);
        let mut end = shown[at..].find('\n').map_or(shown.len(), |i| at + i);
        start = start.max(floor_char(shown, at.saturating_sub(REACH)));
        end = end.min(floor_char(shown, (selection.end + REACH).min(shown.len())));
        let end = end.max(selection.end.min(shown.len()));
        let local =
            |r: &Range<usize>| r.start.clamp(start, end) - start..r.end.clamp(start, end) - start;
        let to_window = |r: Rect| r.translate(origin.x, origin.y);
        let caret = text.caret(layout, shown_base + selection.end);
        Self {
            text: shown[start..end].to_owned(),
            base: shown_base + start,
            selection: local(&selection),
            composition: composition.as_ref().map(local),
            clusters: text
                .clusters(layout, shown_base + start..shown_base + end)
                .into_iter()
                .map(|(bytes, rect)| {
                    let bytes = bytes.start - shown_base..bytes.end - shown_base;
                    (local(&bytes), to_window(rect))
                })
                .collect(),
            caret: to_window(Rect::new(caret.x, caret.top, 1.0, caret.height)),
        }
    }

    pub fn translate(mut self, dx: f32, dy: f32) -> Self {
        for (_, rect) in &mut self.clusters {
            *rect = rect.translate(dx, dy);
        }
        self.caret = self.caret.translate(dx, dy);
        self
    }

    /// The box of the first line of bytes `range`, from its first character to its
    /// last one on that line, and the bytes that box covers (all of `range` only when
    /// it fits on one line). An empty range is a caret's width at its start.
    pub fn first_rect(&self, range: Range<usize>) -> (Rect, Range<usize>) {
        let mut first: Option<(Rect, Range<usize>)> = None;
        for (bytes, rect) in &self.clusters {
            if range.is_empty() {
                if bytes.start >= range.start {
                    return (Rect::new(rect.x, rect.y, 0.0, rect.height), range);
                }
                continue;
            }
            if bytes.end <= range.start || bytes.start >= range.end {
                continue;
            }
            match &mut first {
                None => first = Some((*rect, bytes.start.max(range.start)..bytes.end)),
                Some((line, covered)) if rect.y == line.y => {
                    *line = line.union(rect);
                    covered.end = bytes.end;
                }
                Some(_) => break,
            }
        }
        if let Some((rect, covered)) = first {
            return (rect, covered.start..covered.end.min(range.end));
        }
        let rect = match self.clusters.last() {
            // At the end of the text: just after the last character.
            Some((bytes, rect)) if range.start >= bytes.end => {
                Rect::new(rect.right(), rect.y, 0.0, rect.height)
            }
            _ => self.caret,
        };
        (rect, range.start..range.start)
    }

    /// The byte index of the character under `pos`, or `None` when no character is
    /// there.
    pub fn index_at(&self, pos: Point) -> Option<usize> {
        self.clusters
            .iter()
            .find(|(_, rect)| rect.contains(pos))
            .map(|(bytes, _)| bytes.start)
    }
}

fn floor_char(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use noa_gpui_text::{FontOptions, TextStyle};

    fn store() -> TextStore {
        TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        })
    }

    #[test]
    fn the_context_is_the_paragraph_around_the_caret() {
        let mut text = store();
        let shown = "first\nsecond line\nthird";
        let layout = text.create(shown, TextStyle::new(14.0), None);
        let origin = Point::new(10.0, 20.0);
        // "line" is selected.
        let context = ImeContext::of_layout(&text, layout, (shown, 0), 13..17, None, origin);
        assert_eq!(context.text, "second line");
        assert_eq!(context.base, 6);
        assert_eq!(context.selection, 7..11);
        assert_eq!(context.clusters.len(), "second line".len());
        let (line, covered) = context.first_rect(7..11);
        assert_eq!(covered, 7..11);
        let caret = |i| text.caret(layout, i);
        assert_eq!(line.x, origin.x + caret(13).x);
        assert_eq!(line.right(), origin.x + caret(17).x);
        assert_eq!(line.y, origin.y + caret(13).top);
        let middle = Point::new(line.x + line.width / 2.0, line.y + 2.0);
        assert_eq!(context.index_at(middle), Some(9));
        assert_eq!(context.index_at(Point::new(-50.0, -50.0)), None);
        // An empty range at the end sits after the last character.
        let (end, _) = context.first_rect(11..11);
        assert_eq!(end.x, origin.x + caret(17).x);
        assert_eq!(end.width, 0.0);
    }

    #[test]
    fn a_long_paragraph_is_cut_around_the_caret() {
        let mut text = store();
        let shown = "あ".repeat(3000);
        let layout = text.create(&shown, TextStyle::new(14.0), None);
        let caret = 3 * 1500;
        let context = ImeContext::of_layout(
            &text,
            layout,
            (&shown, 0),
            caret..caret,
            None,
            Point::default(),
        );
        assert!(context.text.len() <= 2 * REACH);
        assert_eq!(context.base + context.selection.start, caret);
        assert!(shown.is_char_boundary(context.base));
    }
}
