use noa_gpui_layout::style::{Size as LayoutSize, auto, length};
use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Point, Rect, Size, TextLayoutId};
use noa_gpui_text::{TextAlign, TextStore, TextStyle};
use unicode_segmentation::UnicodeSegmentation;

use super::CachedText;
use super::editable::Editable;
use crate::event::{Cursor, Event};
use crate::styling::{StylePatch, TextProperty};
use crate::text_edit::TextEditor;
use crate::theme::{Paint, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Where a [`Label`] (or a [`super::TableColumn`]'s cells) that does not fit is cut,
/// with an ellipsis in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Truncation {
    /// Never cut (the label overflows or wraps).
    #[default]
    None,
    /// "A long sente…"
    Tail,
    /// "A long…tence"
    Middle,
    /// "…ong sentence"
    Head,
}

const ELLIPSIS: &str = "\u{2026}";

/// Static text (`NSTextField` as a label). Single-line by default. A truncated label
/// shrinks with its container; selectable labels are not truncated.
#[derive(Debug)]
pub struct Label {
    text: String,
    style: TextStyle,
    color: Paint,
    wrap: bool,
    max_lines: Option<u32>,
    truncation: Truncation,
    shaped: CachedText,
    /// The text actually shown (shortened when truncated).
    shown: String,
    /// Present when selectable; boxed, as most labels are not and it is large.
    select: Option<Box<Editable>>,
    select_width: f32,
    /// The font family by name, resolved when measuring.
    family: Option<String>,
    shaped_style: Option<TextStyle>,
    /// What [`Label::fit`] found for the last few widths; boxed, and only made for a
    /// label that truncates, to keep the others small.
    fits: Option<Box<FitCache>>,
    /// The text properties the builder or `set_*` set, one [`TextProperty::bit`] each.
    own: u8,
}

/// How many widths [`FitCache`] remembers: one layout measures a label at a few (its
/// min-content width, the width it is given, ...).
const FIT_WIDTHS: usize = 4;

/// Results of [`Label::fit`], which shapes the text several times to find where to cut
/// it, while what they depend on stays the same.
#[derive(Debug, Default)]
struct FitCache {
    key: Option<(String, TextStyle, Truncation, Option<u32>)>,
    full_width: Option<f32>,
    /// `(width bits, shortened text)`, the most recent last.
    results: Vec<(u32, Option<String>)>,
    /// The candidates shaped while shortening the text to one line, at any width.
    sizes: CandidateSizes,
}

impl FitCache {
    fn validate(
        &mut self,
        text: &str,
        style: TextStyle,
        truncation: Truncation,
        lines: Option<u32>,
    ) {
        let current = self.key.as_ref().is_some_and(|(t, s, tr, l)| {
            t == text && *s == style && *tr == truncation && *l == lines
        });
        if !current {
            self.key = Some((text.to_owned(), style, truncation, lines));
            self.full_width = None;
            self.results.clear();
            self.sizes = CandidateSizes::default();
        }
    }

    fn get(&self, width: f32) -> Option<&Option<String>> {
        let bits = width.to_bits();
        self.results
            .iter()
            .find(|(w, _)| *w == bits)
            .map(|(_, r)| r)
    }

    fn put(&mut self, width: f32, result: Option<String>) {
        if self.results.len() == FIT_WIDTHS {
            self.results.remove(0);
        }
        self.results.push((width.to_bits(), result));
    }
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TextStyle::new(14.0),
            color: Paint::Role(Role::Text),
            wrap: false,
            max_lines: None,
            truncation: Truncation::None,
            shaped: CachedText::default(),
            shown: String::new(),
            select: None,
            select_width: 0.0,
            family: None,
            shaped_style: None,
            fits: None,
            own: 0,
        }
    }

    /// A stylesheet value for `property`, unless it only reached the label from an
    /// ancestor and the label's builder set the property itself: inherited values rank
    /// below the widget's own settings.
    fn styled<T>(
        &self,
        styling: &StylePatch,
        property: TextProperty,
        value: Option<T>,
    ) -> Option<T> {
        value.filter(|_| !(styling.is_inherited(property) && self.own & property.bit() != 0))
    }

    pub fn font_size(mut self, px: f32) -> Self {
        self.own |= TextProperty::FontSize.bit();
        self.style = TextStyle {
            font_size: px,
            line_height: TextStyle::new(px).line_height,
            ..self.style
        };
        self.sync_select_style();
        self
    }

    /// Sans-serif by default.
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    pub fn italic(mut self, italic: bool) -> Self {
        self.style = self.style.italic(italic);
        self.sync_select_style();
        self
    }

    /// A `None` family means the default.
    pub fn set_font(
        &mut self,
        cx: &mut UpdateCx,
        family: Option<String>,
        size: f32,
        weight: u16,
        italic: bool,
    ) {
        let style = TextStyle {
            family: self.style.family,
            ..TextStyle::new(size)
                .weight(weight)
                .italic(italic)
                .align(self.style.align)
        };
        let own = self.own | TextProperty::FontSize.bit() | TextProperty::FontWeight.bit();
        if family != self.family || style != self.style || own != self.own {
            self.own = own;
            self.family = family;
            self.style = style;
            self.sync_select_style();
            cx.request_layout();
        }
    }

    pub fn weight(mut self, weight: u16) -> Self {
        self.own |= TextProperty::FontWeight.bit();
        self.style = self.style.weight(weight);
        self.sync_select_style();
        self
    }

    pub fn color(mut self, color: impl Into<Paint>) -> Self {
        self.own |= TextProperty::Color.bit();
        self.color = color.into();
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// The last line ends with an ellipsis if cut.
    pub fn max_lines(mut self, lines: u32) -> Self {
        self.wrap = true;
        self.max_lines = Some(lines.max(1));
        self
    }

    /// Line the lines up by their centers or ends (wrapped lines and lines separated by
    /// `\n`), and place the text that way in a label wider than it.
    pub fn align(mut self, align: TextAlign) -> Self {
        self.own |= TextProperty::TextAlign.bit();
        self.style = self.style.align(align);
        self.sync_select_style();
        self
    }

    pub fn truncation(mut self, truncation: Truncation) -> Self {
        self.truncation = truncation;
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.select = selectable.then(|| {
            let mut editor = TextEditor::multi_line();
            editor.set_text(&self.text);
            editor.move_to(0, false);
            let mut edit = Editable::new(editor, String::new(), self.style);
            edit.read_only = true;
            Box::new(edit)
        });
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn shown_text(&self) -> &str {
        &self.shown
    }

    /// The selected byte range of a selectable label.
    pub fn selection(&self) -> Option<std::ops::Range<usize>> {
        self.select.as_ref().map(|e| e.editor.selection())
    }

    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        let text = text.into();
        if text != self.text {
            self.text = text;
            if let Some(edit) = &mut self.select {
                edit.editor.set_text(&self.text);
                edit.editor.move_to(0, false);
            }
            cx.request_layout();
        }
    }

    pub fn set_color(&mut self, cx: &mut UpdateCx, color: impl Into<Paint>) {
        let color = color.into();
        // Becoming the label's own outranks an inherited color even when the value
        // stays the same.
        let own = self.own | TextProperty::Color.bit();
        if color != self.color || own != self.own {
            self.color = color;
            self.own = own;
            cx.request_paint();
        }
    }

    fn text_x(&self, width: f32, text_width: f32) -> f32 {
        let room = (width - text_width).max(0.0);
        // The measured style carries a stylesheet's `text-align`.
        match self
            .shaped_style
            .map_or(self.style.align, |style| style.align)
        {
            TextAlign::Start => 0.0,
            TextAlign::Center => (room / 2.0).round(),
            TextAlign::End => room,
        }
    }

    fn sync_select_style(&mut self) {
        if let Some(edit) = &mut self.select {
            edit.style = self.style;
        }
    }

    fn truncates(&self) -> bool {
        self.select.is_none() && (self.truncation != Truncation::None || self.max_lines.is_some())
    }

    /// The longest shortened form of the text, built by `candidate(kept graphemes)`,
    /// that `fits`.
    fn shorten(
        store: &mut TextStore,
        graphemes: &[&str],
        style: TextStyle,
        wrap: Option<f32>,
        candidate: impl Fn(usize) -> String,
        fits: impl Fn(Size) -> bool,
    ) -> String {
        let (text, layout) =
            Self::shorten_kept(store, graphemes, style, wrap, candidate, fits, None);
        if let Some(id) = layout {
            store.remove(id);
        }
        text
    }

    /// [`Label::shorten`], also handing back the layout (`wrap` wide) of the shortened
    /// text when the search shaped it: the caller owns it. `sizes`, if given, holds
    /// candidates measured before (of this text, `style`, `wrap` and `candidate`); the
    /// search reads them instead of shaping those again, and adds the ones it shapes.
    fn shorten_kept(
        store: &mut TextStore,
        graphemes: &[&str],
        style: TextStyle,
        wrap: Option<f32>,
        candidate: impl Fn(usize) -> String,
        fits: impl Fn(Size) -> bool,
        mut sizes: Option<&mut CandidateSizes>,
    ) -> (String, Option<TextLayoutId>) {
        // The layout of the longest candidate found to fit so far (that of `low`), if
        // this search shaped it.
        let mut kept: Option<TextLayoutId> = None;
        // Binary search on how many graphemes to keep. Known sizes answer the same as
        // shaping, so the search probes the same candidates and ends at the same one.
        let (mut low, mut high) = (0, graphemes.len());
        while low < high {
            let mid = (low + high).div_ceil(2);
            let (size, id) = match sizes.as_deref().and_then(|sizes| sizes.get(mid)) {
                Some(size) => (size, None),
                None => {
                    let id = store.create(&candidate(mid), style, wrap);
                    let size = store.size(id);
                    if let Some(sizes) = sizes.as_deref_mut() {
                        sizes.put(mid, size);
                    }
                    (size, Some(id))
                }
            };
            if fits(size) {
                low = mid;
                if let Some(old) = std::mem::replace(&mut kept, id) {
                    store.remove(old);
                }
            } else {
                high = mid - 1;
                if let Some(id) = id {
                    store.remove(id);
                }
            }
        }
        (candidate(low), kept)
    }

    /// The shortened text to show in `width` (and the line limit), or `None` when the
    /// whole text is shown.
    fn fit(
        &mut self,
        store: &mut TextStore,
        width: Option<f32>,
        style: TextStyle,
    ) -> Option<String> {
        let width = width.filter(|_| self.truncates())?;
        self.fit_truncating(store, width, style)
    }

    /// [`Label::fit`] for a label that truncates. Apart, so that measuring the many
    /// labels that do not stays short.
    #[inline(never)]
    fn fit_truncating(
        &mut self,
        store: &mut TextStore,
        width: f32,
        style: TextStyle,
    ) -> Option<String> {
        let mut fits = self.fits.take().unwrap_or_default();
        fits.validate(&self.text, style, self.truncation, self.max_lines);
        let result = match fits.get(width) {
            Some(result) => result.clone(),
            None => {
                let FitCache {
                    full_width, sizes, ..
                } = &mut *fits;
                let result = self.compute_fit(store, width, full_width, sizes, style);
                fits.put(width, result.clone());
                result
            }
        };
        self.fits = Some(fits);
        result
    }

    /// [`Label::fit`] for a label that truncates, without the cache. `full_width` is the
    /// text's width on one line, if known (and is set when found); `sizes` are the
    /// candidates measured for one line so far (and get those this search shapes).
    fn compute_fit(
        &self,
        store: &mut TextStore,
        width: f32,
        full_width: &mut Option<f32>,
        sizes: &mut CandidateSizes,
        style: TextStyle,
    ) -> Option<String> {
        if let Some(lines) = self.max_lines {
            let limit = style.line_height * lines as f32 + 0.5;
            let full = {
                let id = store.create(&self.text, style, Some(width));
                let size = store.size(id);
                store.remove(id);
                size
            };
            if full.height <= limit {
                return None;
            }
            let graphemes: Vec<&str> = self.text.graphemes(true).collect();
            return Some(Self::shorten(
                store,
                &graphemes,
                style,
                Some(width),
                |k| format!("{}{ELLIPSIS}", graphemes[..k].concat().trim_end()),
                |size| size.height <= limit,
            ));
        }
        // The same at every width: shaped once per text and style.
        let full_width = match *full_width {
            Some(w) => w,
            None => {
                let id = store.create(&self.text, style, None);
                let w = store.size(id).width;
                store.remove(id);
                *full_width = Some(w);
                w
            }
        };
        fit_measured_line(
            store,
            &self.text,
            full_width,
            style,
            width,
            self.truncation,
            sizes,
        )
    }
}

/// `text`, measured `full_width` wide on one line, shortened with an ellipsis where
/// `truncation` says (the end for [`Truncation::None`]) to fit on one line `width`
/// wide, or `None` when it fits whole. Shared with the cells of a [`super::TableView`].
/// `sizes` as for [`fit_measured_line_kept`].
pub(crate) fn fit_measured_line(
    store: &mut TextStore,
    text: &str,
    full_width: f32,
    style: TextStyle,
    width: f32,
    truncation: Truncation,
    sizes: &mut CandidateSizes,
) -> Option<String> {
    let (shown, layout) =
        fit_measured_line_kept(store, text, full_width, style, width, truncation, sizes)?;
    if let Some(id) = layout {
        store.remove(id);
    }
    Some(shown)
}

/// [`fit_measured_line`], also handing back the one-line layout of the shortened text
/// when finding it shaped it: the caller owns it. `sizes` holds the candidates of
/// `text` in `style` shortened as `truncation` says that were measured before, for
/// another `width`: they are not shaped again (see [`CandidateSizes`]).
pub(crate) fn fit_measured_line_kept(
    store: &mut TextStore,
    text: &str,
    full_width: f32,
    style: TextStyle,
    width: f32,
    truncation: Truncation,
    sizes: &mut CandidateSizes,
) -> Option<(String, Option<TextLayoutId>)> {
    if full_width <= width + 0.5 {
        return None;
    }
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let n = graphemes.len();
    let candidate = |k: usize| match truncation {
        Truncation::Head => format!("{ELLIPSIS}{}", graphemes[n - k..].concat()),
        Truncation::Middle => {
            let front = k.div_ceil(2);
            let back = k / 2;
            format!(
                "{}{ELLIPSIS}{}",
                graphemes[..front].concat(),
                graphemes[n - back..].concat()
            )
        }
        _ => format!("{}{ELLIPSIS}", graphemes[..k].concat()),
    };
    Some(Label::shorten_kept(
        store,
        &graphemes,
        style,
        None,
        candidate,
        |size| size.width <= width + 0.5,
        Some(sizes),
    ))
}

/// The sizes of the shortened forms of one text that were shaped, by how many of its
/// graphemes each keeps, so that shortening the text again for another width (a
/// column being resized) does not shape the same candidates again. Valid only for the
/// text, style and truncation they were measured with.
#[derive(Debug, Default)]
pub(crate) struct CandidateSizes(Vec<(usize, Size)>);

impl CandidateSizes {
    /// How many sizes are kept at most: a resize's searches keep finding the same few
    /// candidates near the cut, and a long text has many more.
    const MAX: usize = 32;

    fn get(&self, kept: usize) -> Option<Size> {
        self.0
            .iter()
            .find(|&&(k, _)| k == kept)
            .map(|&(_, size)| size)
    }

    fn put(&mut self, kept: usize, size: Size) {
        if self.0.len() < Self::MAX {
            self.0.push((kept, size));
        }
    }
}

impl Widget for Label {
    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        self.select.as_ref().map(|_| Cursor::Text)
    }

    fn style(&self) -> Style {
        let mut style = Style {
            flex_shrink: if self.wrap || self.truncates() {
                1.0
            } else {
                0.0
            },
            ..Default::default()
        };
        if self.truncates() {
            style.min_size = LayoutSize {
                width: length(0.0),
                height: auto(),
            };
        }
        style
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        let mut style = self.style;
        style.family = self.family.as_deref().map(|f| cx.text.font_family(f));
        let styling = cx.styling();
        if let Some(size) = self.styled(styling, TextProperty::FontSize, styling.font_size) {
            style.font_size = size;
            style.line_height = TextStyle::new(size).line_height;
        }
        if let Some(weight) = self.styled(styling, TextProperty::FontWeight, styling.font_weight) {
            style = style.weight(weight);
        }
        if let Some(align) = self.styled(styling, TextProperty::TextAlign, styling.text_align) {
            style = style.align(align);
        }
        if let Some(edit) = &mut self.select {
            edit.style = style;
        }
        if self.shaped_style != Some(style) {
            self.shaped.release(cx.text);
            self.shaped_style = Some(style);
        }
        let available = input.known.width.or(input.wrap_width());
        let wrap = if self.wrap { input.wrap_width() } else { None };
        if let Some(edit) = &mut self.select {
            self.shown.clone_from(&self.text);
            let size = edit.shape(cx.text, wrap);
            self.select_width = size.width;
            return size;
        }
        // Measuring runs several times per layout: reuse `shown` when it is unchanged.
        match self.fit(cx.text, available, style) {
            Some(short) => self.shown = short,
            None if self.shown != self.text => self.shown.clone_from(&self.text),
            None => {}
        }
        let size = self.shaped.shape(cx.text, &self.shown, style, wrap);
        match available.filter(|_| self.truncates()) {
            Some(width) => Size::new(size.width.min(width), size.height),
            None => size,
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let styling = cx.styling();
        let color = self.styled(styling, TextProperty::Color, styling.color);
        let color = cx.color(color.unwrap_or(self.color));
        if let Some(edit) = &self.select {
            let focused = cx.is_focused();
            let origin = Point::new(self.text_x(cx.size().width, self.select_width), 0.0);
            edit.paint_colored(cx, origin, focused, color);
            return;
        }
        if let Some(layout) = self.shaped.layout() {
            let size = self.shaped.size();
            let x = self.text_x(cx.size().width, size.width);
            cx.text(Rect::new(x, 0.0, size.width, size.height), layout, color);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let origin = Point::new(self.text_x(cx.size().width, self.select_width), 0.0);
        if let Some(edit) = &mut self.select {
            edit.event(cx, event, origin);
        }
    }

    fn focusable(&self) -> bool {
        self.select.is_some()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Label)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_value(self.text.clone());
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
        if let Some(edit) = &mut self.select {
            edit.removed(text);
        }
    }
}
