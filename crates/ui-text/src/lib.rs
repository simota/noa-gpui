//! Text is shaped once with cosmic-text and kept in a [`TextStore`]. Draw commands
//! refer to a layout by [`TextLayoutId`], so the display list stays free of text and
//! GPU types. Layout sizes are in logical pixels.

pub use cosmic_text;

use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use cosmic_text::{
    Align, Attrs, Buffer, Cursor, Family, FontSystem, Metrics, Shaping, Style, Weight, fontdb,
};
use noa_gpui_scene::{Rect, Size, TextLayoutId};

/// The font bundled with the framework (Inter, SIL Open Font License 1.1).
///
/// Lets Latin text render on platforms without system fonts, such as the Web.
pub const BUNDLED_FONT: &[u8] = include_bytes!("../assets/fonts/Inter.ttf");
const BUNDLED_FAMILY: &str = "Inter";

#[derive(Debug, Clone)]
pub struct FontOptions {
    /// BCP 47 locale used for font fallback (for example `"ja-JP"`).
    pub locale: String,
    /// Needed for scripts the bundled font lacks, such as Japanese.
    pub load_system_fonts: bool,
}

impl Default for FontOptions {
    /// The operating system's locale (falling back to `en-US`) and, on native targets,
    /// the system fonts. The locale picks regional glyph variants for shared CJK
    /// characters, so Japanese text falls back to a Japanese font.
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let locale = sys_locale::get_locale().unwrap_or_else(|| "en-US".to_owned());
        #[cfg(target_arch = "wasm32")]
        let locale = "en-US".to_owned();
        Self {
            locale,
            load_system_fonts: !cfg!(target_arch = "wasm32"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontFamily(u32);

/// Start is the default. The block is as wide as its widest line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Line height in logical pixels.
    pub line_height: f32,
    /// CSS-style weight, 100..=900.
    pub weight: u16,
    /// The family; `None` is the default sans-serif (the bundled font).
    pub family: Option<FontFamily>,
    pub italic: bool,
    pub align: TextAlign,
}

impl TextStyle {
    pub fn new(font_size: f32) -> Self {
        Self {
            font_size,
            line_height: (font_size * 1.3).ceil(),
            weight: 400,
            family: None,
            italic: false,
            align: TextAlign::Start,
        }
    }

    pub fn align(self, align: TextAlign) -> Self {
        Self { align, ..self }
    }

    pub fn weight(self, weight: u16) -> Self {
        Self { weight, ..self }
    }

    pub fn family(self, family: FontFamily) -> Self {
        Self {
            family: Some(family),
            ..self
        }
    }

    pub fn italic(self, italic: bool) -> Self {
        Self { italic, ..self }
    }
}

struct Layout {
    buffer: Buffer,
    size: Size,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretPosition {
    pub x: f32,
    pub top: f32,
    pub height: f32,
}

impl Layout {
    /// Byte offset of each paragraph (cosmic-text `BufferLine`) in the original text.
    fn paragraph_starts(&self) -> Vec<usize> {
        let mut starts = Vec::with_capacity(self.buffer.lines.len());
        let mut offset = 0;
        for line in &self.buffer.lines {
            starts.push(offset);
            offset += line.text().len() + line.ending().as_str().len();
        }
        starts
    }

    /// `index` is clamped; the cursor is in the last paragraph that starts at or before it.
    fn cursor(&self, index: usize) -> Cursor {
        // Walk the paragraphs up to `index` instead of collecting every start: callers
        // ask for many cursors in long texts (one per search match).
        let (mut line, mut start) = (0, 0);
        let mut offset = 0;
        for (i, l) in self.buffer.lines.iter().enumerate() {
            if offset > index {
                break;
            }
            (line, start) = (i, offset);
            offset += l.text().len() + l.ending().as_str().len();
        }
        let text_len = self.buffer.lines.get(line).map_or(0, |l| l.text().len());
        Cursor::new(line, (index - start).min(text_len))
    }

    fn offset(&self, cursor: Cursor) -> usize {
        let lines = &self.buffer.lines;
        if cursor.line >= lines.len() {
            return 0;
        }
        let start: usize = lines[..cursor.line]
            .iter()
            .map(|l| l.text().len() + l.ending().as_str().len())
            .sum();
        start + cursor.index
    }
}

#[derive(Default)]
pub struct TextLayouts {
    slots: Vec<Option<Layout>>,
    free: Vec<u32>,
    /// Zero until read after a modification; nonzero revisions identify content
    /// across stores, since layout IDs themselves are local to one store.
    revision: AtomicU64,
    changes: u64,
}

static NEXT_LAYOUT_REVISION: AtomicU64 = AtomicU64::new(1);

impl TextLayouts {
    /// # Panics
    /// If `id` was removed or never created.
    pub fn buffer(&self, id: TextLayoutId) -> &Buffer {
        &self.get(id).buffer
    }

    /// In logical pixels.
    pub fn size(&self, id: TextLayoutId) -> Size {
        self.get(id).size
    }

    /// Horizontal position (logical pixels) of the caret before byte `index` of the
    /// first line. Positions past the end map to the end of the line.
    pub fn caret_x(&self, id: TextLayoutId, index: usize) -> f32 {
        let Some(run) = self.get(id).buffer.layout_runs().next() else {
            return 0.0;
        };
        for glyph in run.glyphs {
            if index <= glyph.start {
                return glyph.x;
            }
            if index < glyph.end {
                // Inside a multi-byte cluster (ligature): interpolate across it.
                let t = (index - glyph.start) as f32 / (glyph.end - glyph.start) as f32;
                return glyph.x + glyph.w * t;
            }
        }
        run.line_w
    }

    /// Caret geometry before byte `index` of the text (all lines; positions past the end
    /// map to the end). At a soft wrap the caret goes to the start of the next line.
    pub fn caret(&self, id: TextLayoutId, index: usize) -> CaretPosition {
        let layout = self.get(id);
        let cursor = layout.cursor(index);
        let line_height = layout.buffer.metrics().line_height;
        let mut found = None;
        for run in layout.buffer.layout_runs() {
            // Runs come in paragraph order.
            if run.line_i > cursor.line {
                break;
            }
            if run.line_i != cursor.line {
                continue;
            }
            // A cursor at the end of a wrapped run also matches the next run's start;
            // prefer the later run, except at the very end of the paragraph.
            let ends_here = run.glyphs.last().is_some_and(|g| g.end == cursor.index);
            if let Some(x) = run.cursor_position(&cursor) {
                found = Some(CaretPosition {
                    x,
                    top: run.line_top,
                    height: run.line_height,
                });
                if !ends_here {
                    break;
                }
            }
        }
        found.unwrap_or(CaretPosition {
            x: 0.0,
            top: 0.0,
            height: line_height,
        })
    }

    /// The byte index nearest to the point `(x, y)` (layout coordinates).
    pub fn hit(&self, id: TextLayoutId, x: f32, y: f32) -> usize {
        let layout = self.get(id);
        match layout.buffer.hit(x, y.max(0.0)) {
            Some(cursor) => layout.offset(cursor),
            None => layout
                .paragraph_starts()
                .last()
                .zip(layout.buffer.lines.last())
                .map_or(0, |(start, line)| start + line.text().len()),
        }
    }

    /// The glyph clusters overlapping bytes `range`, in text order, each with the bytes
    /// it shows and its box (a line high): what an input method asks to place its
    /// candidate window or to find the character under the pointer.
    pub fn clusters(&self, id: TextLayoutId, range: Range<usize>) -> Vec<(Range<usize>, Rect)> {
        let layout = self.get(id);
        let lines = &layout.buffer.lines;
        // Paragraph starts, counted up as the runs reach them: the paragraphs before
        // `range` cost an addition each, and those after it nothing.
        let (mut line, mut base) = (0, 0);
        let mut clusters = Vec::new();
        for run in layout.buffer.layout_runs() {
            while line < run.line_i {
                base += lines[line].text().len() + lines[line].ending().as_str().len();
                line += 1;
            }
            if base > range.end {
                break;
            }
            if base + lines[line].text().len() < range.start {
                continue;
            }
            for glyph in run.glyphs {
                let bytes = base + glyph.start..base + glyph.end;
                if bytes.end <= range.start || bytes.start >= range.end {
                    continue;
                }
                // Ligatures and combined characters are one glyph over several
                // characters; a cluster shaped into several glyphs comes once.
                if clusters
                    .last()
                    .is_some_and(|(last, _): &(Range<usize>, Rect)| *last == bytes)
                {
                    continue;
                }
                let rect = Rect::new(glyph.x, run.line_top, glyph.w, run.line_height);
                clusters.push((bytes, rect));
            }
        }
        clusters.sort_by_key(|(bytes, _)| bytes.start);
        clusters
    }

    /// Rectangles covering bytes `start..end`, one or more per visual line.
    pub fn selection_rects(&self, id: TextLayoutId, start: usize, end: usize) -> Vec<Rect> {
        let layout = self.get(id);
        let (from, to) = (layout.cursor(start.min(end)), layout.cursor(start.max(end)));
        let mut rects = Vec::new();
        for run in layout.buffer.layout_runs() {
            // Runs come in paragraph order.
            if run.line_i > to.line {
                break;
            }
            if run.line_i < from.line {
                continue;
            }
            for (x, width) in run.highlight(from, to) {
                rects.push(Rect::new(x, run.line_top, width, run.line_height));
            }
            // A selected line break shows as a sliver past the end of the line.
            let ends_paragraph = run.glyphs.last().is_none_or(|g| g.end == run.text.len());
            if ends_paragraph && run.line_i >= from.line && run.line_i < to.line {
                rects.push(Rect::new(run.line_w, run.line_top, 6.0, run.line_height));
            }
        }
        rects
    }

    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    /// Changes whenever a layout is created, rewrapped or removed, and differs
    /// between stores. Equal revisions identify the same layout content.
    pub fn revision(&self) -> u64 {
        let current = self.revision.load(Ordering::Relaxed);
        if current != 0 {
            return current;
        }
        // Assign once per observed change, rather than for every label shaped
        // while building a frame. Concurrent readers keep the same revision.
        let fresh = NEXT_LAYOUT_REVISION.fetch_add(1, Ordering::Relaxed);
        match self
            .revision
            .compare_exchange(0, fresh, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => fresh,
            Err(numbered) => numbered,
        }
    }

    /// How many times a layout was created, rewrapped or removed in this store.
    pub fn changes(&self) -> u64 {
        self.changes
    }

    fn modified(&mut self) {
        *self.revision.get_mut() = 0;
        self.changes += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, id: TextLayoutId) -> &Layout {
        self.slots[id.0 as usize]
            .as_ref()
            .expect("text layout was removed")
    }

    fn insert(&mut self, layout: Layout) -> TextLayoutId {
        self.modified();
        match self.free.pop() {
            Some(index) => {
                self.slots[index as usize] = Some(layout);
                TextLayoutId(index)
            }
            None => {
                self.slots.push(Some(layout));
                TextLayoutId(self.slots.len() as u32 - 1)
            }
        }
    }
}

pub struct TextStore {
    font_system: FontSystem,
    layouts: TextLayouts,
    family_names: Vec<String>,
    locale: String,
    /// Hints for copying a layout instead of shaping the same text again (labels, table
    /// cells and list rows repeat): indexed by the hash of text, style and wrap width, each
    /// holds the layout last made of them, or [`NO_LAYOUT`]. [`TextStore::reusable`]
    /// checks that the layout really holds that text in that style and width. Small, as
    /// no layout keeps a key: see `recent_place`.
    recent: Vec<u32>,
}

const NO_LAYOUT: u32 = u32::MAX;

/// Fixed-width families to stand for `monospace`, most preferred first.
const MONOSPACE_FAMILIES: &[&str] = if cfg!(target_os = "macos") {
    &["SF Mono", "Menlo", "Monaco"]
} else if cfg!(windows) {
    &["Cascadia Mono", "Consolas"]
} else {
    &[
        "DejaVu Sans Mono",
        "Noto Sans Mono",
        "Liberation Mono",
        "Ubuntu Mono",
    ]
};

fn first_loaded<'a>(db: &fontdb::Database, names: &[&'a str]) -> Option<&'a str> {
    names.iter().copied().find(|name| {
        db.faces()
            .any(|face| face.families.iter().any(|(family, _)| family == name))
    })
}

impl TextStore {
    pub fn new(options: FontOptions) -> Self {
        let mut db = fontdb::Database::new();
        // The font is part of the program: read it where it is rather than from a copy.
        db.load_font_source(fontdb::Source::Binary(std::sync::Arc::new(BUNDLED_FONT)));
        if options.load_system_fonts {
            db.load_system_fonts();
        }
        db.set_sans_serif_family(BUNDLED_FAMILY);
        // fontdb's own choice is Courier New, which is not what a terminal or code view
        // on the system looks like.
        if let Some(name) = first_loaded(&db, MONOSPACE_FAMILIES) {
            db.set_monospace_family(name);
        }
        Self {
            font_system: FontSystem::new_with_locale_and_db(options.locale.clone(), db),
            layouts: TextLayouts::default(),
            family_names: Vec::new(),
            locale: options.locale,
            recent: Vec::new(),
        }
    }

    /// Add a font (TrueType or OpenType data) to fall back to for characters the other
    /// fonts lack, such as Japanese on the Web, where no system fonts are loaded.
    ///
    /// Load fonts before laying out text: layouts made earlier keep the glyphs they
    /// were shaped with.
    pub fn load_font(&mut self, data: Vec<u8>) {
        self.font_system
            .db_mut()
            .load_font_source(fontdb::Source::Binary(std::sync::Arc::new(data)));
    }

    /// The operating system's locale with [`FontOptions::default`]. Use it to format dates
    /// and numbers the user's way.
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// The names of the loaded font families, sorted and without duplicates.
    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .font_system
            .db()
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            // Hidden system families (".SF NS", ...) are not for choosing.
            .filter(|name| !name.starts_with('.'))
            .collect();
        names.sort_by_cached_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    /// The family named `name`, for [`TextStyle::family`]. Text falls back to other fonts
    /// when no loaded font has that name. `monospace` and `serif` stand for the system's
    /// fixed-width and serif fonts: `monospace` is the first of SF Mono, Menlo and
    /// Monaco on macOS, Cascadia Mono and Consolas on Windows, and DejaVu Sans Mono,
    /// Noto Sans Mono, Liberation Mono and Ubuntu Mono elsewhere that is loaded.
    pub fn font_family(&mut self, name: &str) -> FontFamily {
        // The generic families go by the name of the font they stand for: for generic
        // monospace, cosmic-text prefers monospaced fallback fonts, and may pick one
        // without outlines (macOS's GB18030 Bitmap) whose glyphs have no width.
        let generic = match name {
            "monospace" => Some(fontdb::Family::Monospace),
            "serif" => Some(fontdb::Family::Serif),
            _ => None,
        };
        // Borrowed from the font database: only a newly registered name is copied.
        let name = match generic {
            Some(ref generic) => self.font_system.db().family_name(generic),
            None => name,
        };
        let index = match self.family_names.iter().position(|n| n == name) {
            Some(index) => index,
            None => {
                self.family_names.push(name.to_owned());
                self.family_names.len() - 1
            }
        };
        FontFamily(index as u32)
    }

    pub fn family_name(&self, family: FontFamily) -> &str {
        &self.family_names[family.0 as usize]
    }

    /// Lines wrap at `max_width` logical pixels if given.
    pub fn create(&mut self, text: &str, style: TextStyle, max_width: Option<f32>) -> TextLayoutId {
        let place = self.recent_place(text, style, max_width);
        let attrs = attrs_of(&self.family_names, style);
        // The same text in the same style and width is laid out the same way: copy a
        // layout that holds it rather than shape it again (a tenth of the work).
        if let Some(place) = place
            && let Some(layout) = self.reusable(self.recent[place], text, &attrs, style, max_width)
        {
            let copy = Layout {
                buffer: layout.buffer.clone(),
                size: layout.size,
            };
            return self.layouts.insert(copy);
        }
        // `Buffer::new` would shape an empty text first, only for `set_text` to replace
        // it: start empty, as the text is set and shaped right below.
        let mut buffer = Buffer::new_empty(Metrics::new(style.font_size, style.line_height));
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        let size = lay_out(&mut buffer, &mut self.font_system, style.align, max_width);
        let id = self.layouts.insert(Layout { buffer, size });
        if let Some(place) = place {
            self.recent[place] = id.0;
        }
        id
    }

    /// Where [`TextStore::recent`] keeps a layout of `text` in `style` wrapped at
    /// `max_width`, after making room for one more layout; `None` for text that is not
    /// copied (see [`TextStore::reusable`]).
    fn recent_place(
        &mut self,
        text: &str,
        style: TextStyle,
        max_width: Option<f32>,
    ) -> Option<usize> {
        if style.align != TextAlign::Start {
            return None;
        }
        // A place for every four layouts (at most two bytes each), and 1 KiB in all for
        // a few layouts: repeated texts are mostly made close together (a column's
        // cells, a list's rows). The old places' keys are not known, so they start over
        // empty.
        let wanted = ((self.layouts.len() + 1).next_power_of_two() / 4).max(256);
        if self.recent.len() < wanted {
            // Freed first: never two tables at once.
            self.recent = Vec::new();
            self.recent = vec![NO_LAYOUT; wanted];
        }
        let TextStyle {
            font_size,
            line_height,
            weight,
            family,
            italic,
            align: _,
        } = style;
        let mut hasher = KeyHasher::default();
        (
            text,
            font_size.to_bits(),
            line_height.to_bits(),
            weight,
            family,
            italic,
            max_width.map(f32::to_bits),
        )
            .hash(&mut hasher);
        Some(hasher.finish() as usize & (self.recent.len() - 1))
    }

    /// Layout `id` if it is what [`TextStore::create`] makes of `text` in `style` (whose
    /// attributes are `attrs`) at `max_width`: of start-aligned text, the width the
    /// buffer wraps at is `max_width`, which tells the layout's wrap width.
    fn reusable(
        &self,
        id: u32,
        text: &str,
        attrs: &Attrs,
        style: TextStyle,
        max_width: Option<f32>,
    ) -> Option<&Layout> {
        let layout = self.layouts.slots.get(id as usize)?.as_ref()?;
        let buffer = &layout.buffer;
        // The buffer keeps the width clamped as `lay_out` set it.
        (style.align == TextAlign::Start
            && buffer.size().0 == max_width.map(|w| w.max(0.0))
            && made_with_style(buffer, attrs, style)
            && holds_text(buffer, text))
        .then_some(layout)
    }

    /// Lay the text of `id` out again for another `max_width`, keeping its shaped glyphs
    /// (finding them is most of the work of [`TextStore::create`]). Afterwards the
    /// layout is the one `create` makes for the same text and `style` at `max_width`.
    ///
    /// Returns `false`, changing nothing, when `id` was created with another style:
    /// that text has to be shaped again.
    ///
    /// # Panics
    /// If `id` was removed or never created.
    pub fn rewrap(&mut self, id: TextLayoutId, style: TextStyle, max_width: Option<f32>) -> bool {
        let layout = self.layouts.slots[id.0 as usize]
            .as_ref()
            .expect("text layout was removed");
        if !made_with_style(&layout.buffer, &attrs_of(&self.family_names, style), style) {
            return false;
        }
        let layout = self.layouts.slots[id.0 as usize]
            .as_mut()
            .expect("text layout was removed");
        let buffer = &mut layout.buffer;
        for line in &mut buffer.lines {
            line.set_align(None);
        }
        layout.size = lay_out(buffer, &mut self.font_system, style.align, max_width);
        for line in &mut buffer.lines {
            // A narrower width before this one left room for its many lines: keep no
            // more than a layout made by `create` has.
            let spare = line.layout_opt().is_some_and(|lines| {
                let fresh = match lines.len() {
                    0 | 1 => 1,
                    n => n.next_power_of_two().max(4),
                };
                lines.capacity() > fresh
            });
            if spare {
                *line = line.clone();
            }
        }
        self.layouts.modified();
        true
    }

    /// Drop a layout. Its id may be reused by a later [`TextStore::create`].
    pub fn remove(&mut self, id: TextLayoutId) {
        let slot = &mut self.layouts.slots[id.0 as usize];
        if slot.take().is_some() {
            self.layouts.free.push(id.0);
            self.layouts.modified();
        }
    }

    pub fn size(&self, id: TextLayoutId) -> Size {
        self.layouts.size(id)
    }

    pub fn caret_x(&self, id: TextLayoutId, index: usize) -> f32 {
        self.layouts.caret_x(id, index)
    }

    pub fn caret(&self, id: TextLayoutId, index: usize) -> CaretPosition {
        self.layouts.caret(id, index)
    }

    pub fn hit(&self, id: TextLayoutId, x: f32, y: f32) -> usize {
        self.layouts.hit(id, x, y)
    }

    pub fn selection_rects(&self, id: TextLayoutId, start: usize, end: usize) -> Vec<Rect> {
        self.layouts.selection_rects(id, start, end)
    }

    pub fn clusters(&self, id: TextLayoutId, range: Range<usize>) -> Vec<(Range<usize>, Rect)> {
        self.layouts.clusters(id, range)
    }

    pub fn layouts(&self) -> &TextLayouts {
        &self.layouts
    }

    /// The renderer needs both at once.
    pub fn split_mut(&mut self) -> (&mut FontSystem, &TextLayouts) {
        (&mut self.font_system, &self.layouts)
    }
}

/// The cosmic-text attributes of `style` (its size and alignment are not among them).
fn attrs_of(family_names: &[String], style: TextStyle) -> Attrs<'_> {
    let family = match style.family {
        Some(family) => Family::Name(&family_names[family.0 as usize]),
        None => Family::SansSerif,
    };
    Attrs::new()
        .family(family)
        .weight(Weight(style.weight))
        .style(if style.italic {
            Style::Italic
        } else {
            Style::Normal
        })
}

/// Hashes the key of a layout in [`TextStore::recent`] by mixing in a word at a time.
/// What it hashes may be typed by the user, but colliding keys only take each other's
/// place there: nothing is chained or compared at length.
#[derive(Default)]
struct KeyHasher(u64);

impl KeyHasher {
    fn mix(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

impl Hasher for KeyHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for &chunk in chunks {
            self.mix(u64::from_le_bytes(chunk));
        }
        if !rest.is_empty() {
            let mut word = [0; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.mix(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.mix(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.mix(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.mix(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.mix(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.mix(value as u64);
    }

    fn finish(&self) -> u64 {
        // Places are taken from the low bits: give them the well-mixed high half too.
        self.0 ^ (self.0 >> 32)
    }
}

fn made_with_style(buffer: &Buffer, attrs: &Attrs, style: TextStyle) -> bool {
    buffer.metrics() == Metrics::new(style.font_size, style.line_height)
        && buffer.lines.first().is_some_and(|line| {
            line.align() == line_align(style.align) && line.attrs_list().defaults() == *attrs
        })
}

/// Whether the paragraphs of `buffer`, with their line endings, are exactly `text`.
fn holds_text(buffer: &Buffer, text: &str) -> bool {
    let mut rest = text;
    for line in &buffer.lines {
        let Some(after) = rest
            .strip_prefix(line.text())
            .and_then(|after| after.strip_prefix(line.ending().as_str()))
        else {
            return false;
        };
        rest = after;
    }
    rest.is_empty()
}

/// How cosmic-text lines up the lines of a text aligned as `align`: on its own for
/// [`TextAlign::Start`].
fn line_align(align: TextAlign) -> Option<Align> {
    match align {
        TextAlign::Start => None,
        TextAlign::Center => Some(Align::Center),
        TextAlign::End => Some(Align::End),
    }
}

fn lay_out(
    buffer: &mut Buffer,
    font_system: &mut FontSystem,
    align: TextAlign,
    max_width: Option<f32>,
) -> Size {
    buffer.set_size(max_width, None);
    buffer.shape_until_scroll(font_system, false);
    let mut size = measure(buffer);
    if let Some(align) = line_align(align) {
        // Line the lines up inside the widest one, so that the block is exactly as
        // wide as its text. (Lines that fit the wrap width also fit this.)
        for line in &mut buffer.lines {
            line.set_align(Some(align));
        }
        buffer.set_size(Some(size.width), None);
        buffer.shape_until_scroll(font_system, false);
        size = measure(buffer);
    }
    size
}

fn measure(buffer: &Buffer) -> Size {
    let mut width: f32 = 0.0;
    let mut height: f32 = 0.0;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
    }
    Size::new(width.ceil(), height.ceil())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_key_hashes_preserve_full_words_and_partial_tails() {
        // Captured values: they pin cache placement, so a rewrite of the hasher must
        // reproduce them.
        let bytes: Vec<u8> = (0..=u8::MAX).collect();
        for (len, expected) in [
            (0, 0x0000_0000_0000_0000),
            (1, 0x0000_0000_0000_0000),
            (7, 0xb17e_f0dc_30d8_e5dc),
            (8, 0x447e_f0dc_c5d8_e5dc),
            (9, 0x62b3_8a04_bbc4_aa04),
            (15, 0xe2b7_a68e_a5e6_7b8e),
            (16, 0xa7b7_a68e_e0e6_7b8e),
            (17, 0x748e_5772_2100_4726),
            (63, 0x86d8_90c5_407b_6716),
            (64, 0x5dd8_90c5_9b7b_6716),
            (65, 0xd08c_14f9_4589_4d7e),
            (255, 0x3c7d_f993_6c67_ef94),
            (256, 0xb97d_f993_e967_ef94),
        ] {
            let mut hash = KeyHasher::default();
            hash.write(&bytes[..len]);
            assert_eq!(hash.finish(), expected, "{len} bytes");
        }
    }

    fn store() -> TextStore {
        TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        })
    }

    #[test]
    fn families_are_listed_and_named_families_shape_differently() {
        let mut store = store();
        assert_eq!(store.families(), vec![BUNDLED_FAMILY.to_owned()]);
        let inter = store.font_family(BUNDLED_FAMILY);
        assert_eq!(store.font_family(BUNDLED_FAMILY), inter, "registered once");
        assert_eq!(store.family_name(inter), BUNDLED_FAMILY);
        let plain = store.create("Wide text", TextStyle::new(20.0), None);
        let named = store.create("Wide text", TextStyle::new(20.0).family(inter), None);
        assert_eq!(store.size(plain), store.size(named), "Inter is the default");
        let bold = store.create("Wide text", TextStyle::new(20.0).weight(700), None);
        assert!(store.size(bold).width > store.size(plain).width);
    }

    fn placed(store: &TextStore, id: TextLayoutId) -> String {
        let runs: Vec<String> = store
            .layouts()
            .buffer(id)
            .layout_runs()
            .map(|run| {
                format!(
                    "{} {} {} {} {} {:?}",
                    run.line_i, run.line_y, run.line_top, run.line_height, run.line_w, run.glyphs
                )
            })
            .collect();
        format!("{:?} {}", store.size(id), runs.join("\n"))
    }

    fn line_room(store: &TextStore, id: TextLayoutId) -> Vec<usize> {
        let lines = &store.layouts().buffer(id).lines;
        lines
            .iter()
            .map(|line| line.layout_opt().expect("laid out").capacity())
            .collect()
    }

    #[test]
    fn rewrapping_gives_the_layout_of_a_new_text_at_that_width() {
        let mut store = store();
        let texts = [
            "The quick brown fox jumps over the lazy dog, again and again and again.",
            "one\ntwo words\r\n\nthe fourth paragraph is the longest of them all",
            "left \u{5d0}\u{5d1}\u{5d2} \u{627}\u{644}\u{639} right, office affine fi",
            "",
        ];
        let widths = [
            Some(0.0),
            Some(120.0),
            None,
            Some(63.5),
            Some(0.0),
            Some(4000.0),
            Some(120.0),
        ];
        let aligns = [TextAlign::Start, TextAlign::Center, TextAlign::End];
        for text in texts {
            for align in aligns {
                let style = TextStyle::new(15.0).weight(600).align(align);
                let id = store.create(text, style, None);
                for width in widths {
                    let revision = store.layouts().revision();
                    assert!(store.rewrap(id, style, width));
                    assert_ne!(store.layouts().revision(), revision, "the layout changed");
                    let fresh = store.create(text, style, width);
                    assert_eq!(
                        placed(&store, id),
                        placed(&store, fresh),
                        "{text:?} {align:?} at {width:?}"
                    );
                    for (kept, new) in line_room(&store, id).iter().zip(line_room(&store, fresh)) {
                        assert!(*kept <= new, "{text:?} at {width:?}: {kept} > {new}");
                    }
                    store.remove(fresh);
                }
                store.remove(id);
            }
        }
        assert!(store.layouts().is_empty());
    }

    #[test]
    fn rewrapping_refuses_a_layout_of_another_style() {
        let mut store = store();
        let inter = store.font_family(BUNDLED_FAMILY);
        let style = TextStyle::new(16.0);
        let others = [
            TextStyle::new(17.0),
            TextStyle {
                line_height: 30.0,
                ..style
            },
            style.weight(700),
            style.italic(true),
            style.family(inter),
            style.align(TextAlign::Center),
            style.align(TextAlign::End),
        ];
        for other in others {
            let id = store.create("some words to wrap", style, None);
            let (before, revision) = (placed(&store, id), store.layouts().revision());
            assert!(!store.rewrap(id, other, Some(40.0)), "{other:?}");
            assert_eq!(placed(&store, id), before, "left as it was");
            assert_eq!(store.layouts().revision(), revision);
            let id = store.create("some words to wrap", other, None);
            assert!(!store.rewrap(id, style, Some(40.0)), "{other:?}");
            assert!(store.rewrap(id, other, Some(40.0)), "{other:?}");
        }
    }

    #[test]
    fn measures_single_line() {
        let mut store = store();
        let id = store.create("Hello", TextStyle::new(16.0), None);
        let size = store.size(id);
        assert!(size.width > 20.0 && size.width < 60.0, "{size:?}");
        assert_eq!(size.height, TextStyle::new(16.0).line_height);
    }

    #[test]
    fn wraps_at_max_width() {
        let mut store = store();
        let style = TextStyle::new(16.0);
        let id = store.create("one two three four five six", style, Some(60.0));
        let size = store.size(id);
        assert!(size.width <= 60.0, "{size:?}");
        assert!(size.height >= style.line_height * 3.0, "{size:?}");
    }

    #[test]
    fn carets_hits_and_selections_span_lines() {
        let mut store = store();
        let style = TextStyle::new(16.0);
        let text = "ab\ncd\r\nef";
        let id = store.create(text, style, None);
        let c = |i| store.caret(id, i);
        assert_eq!(c(0).top, 0.0);
        assert_eq!(c(0).x, 0.0);
        assert!(c(2).x > c(1).x);
        // After "ab\n": start of the second line.
        assert_eq!((c(3).x, c(3).top), (0.0, style.line_height));
        // After "cd\r\n" (two-byte ending): start of the third line.
        assert_eq!((c(7).x, c(7).top), (0.0, style.line_height * 2.0));
        assert_eq!(c(99).top, style.line_height * 2.0, "past the end clamps");

        for i in [0, 1, 2, 3, 4, 5, 7, 8, 9] {
            let p = c(i);
            assert_eq!(store.hit(id, p.x + 0.1, p.top + 2.0), i, "byte {i}");
        }
        assert_eq!(store.hit(id, 500.0, 500.0), text.len());

        let rects = store.selection_rects(id, 1, 8);
        let tops: Vec<f32> = rects.iter().map(|r| r.y).collect();
        assert!(tops.contains(&0.0) && tops.contains(&(style.line_height * 2.0)));
        assert!(rects.iter().all(|r| r.width > 0.0));
    }

    #[test]
    fn cursors_and_offsets_agree_with_the_paragraph_starts() {
        let mut store = store();
        let text = "one two three four\n\nfünf sechs sieben\r\n\r\nacht neun zehn elf zwölf\n";
        let id = store.create(text, TextStyle::new(16.0), Some(60.0));
        let layout = store.layouts.get(id);
        let starts = layout.paragraph_starts();
        for index in 0..=text.len() + 2 {
            let line = starts.partition_point(|&s| s <= index) - 1;
            let text_len = layout.buffer.lines[line].text().len();
            let expected = Cursor::new(line, (index - starts[line]).min(text_len));
            let cursor = layout.cursor(index);
            assert_eq!(cursor, expected, "byte {index}");
            assert_eq!(layout.offset(cursor), starts[line] + expected.index);
        }
        let past = Cursor::new(layout.buffer.lines.len(), 0);
        assert_eq!(layout.offset(past), 0, "a paragraph past the end");

        for (start, end) in [(0, 4), (3, 25), (19, 21), (22, text.len()), (40, 41)] {
            let (from, to) = (layout.cursor(start), layout.cursor(end));
            let mut expected = Vec::new();
            for run in layout.buffer.layout_runs() {
                if run.line_i < from.line || run.line_i > to.line {
                    continue;
                }
                for (x, width) in run.highlight(from, to) {
                    expected.push(Rect::new(x, run.line_top, width, run.line_height));
                }
                let ends_paragraph = run.glyphs.last().is_none_or(|g| g.end == run.text.len());
                if ends_paragraph && run.line_i >= from.line && run.line_i < to.line {
                    expected.push(Rect::new(run.line_w, run.line_top, 6.0, run.line_height));
                }
            }
            assert_eq!(
                store.selection_rects(id, start, end),
                expected,
                "{start}..{end}"
            );
        }
    }

    #[test]
    fn soft_wraps_put_the_caret_on_the_next_line() {
        let mut store = store();
        let style = TextStyle::new(16.0);
        let text = "one two three four";
        let id = store.create(text, style, Some(60.0));
        let last = store.caret(id, text.len());
        assert!(last.top >= style.line_height, "{last:?}");
        assert!(store.hit(id, 0.0, last.top + 1.0) > 0);
    }

    #[test]
    fn caret_positions_increase_through_the_line() {
        let mut store = store();
        let text = "Hello";
        let id = store.create(text, TextStyle::new(16.0), None);
        let xs: Vec<f32> = (0..=text.len()).map(|i| store.caret_x(id, i)).collect();
        assert_eq!(xs[0], 0.0);
        assert!(xs.windows(2).all(|w| w[1] > w[0]), "{xs:?}");
        assert!(xs[text.len()] <= store.size(id).width);
        assert_eq!(store.caret_x(id, 99), xs[text.len()]);
    }

    #[test]
    fn generic_monospace_text_falls_back_to_fonts_with_widths() {
        // Needs the system's fonts: on macOS, CJK text in the monospace family could fall
        // back to an outline-less bitmap font and measure infinitely wide.
        let mut store = TextStore::new(FontOptions::default());
        let mono = store.font_family("monospace");
        assert_ne!(store.family_name(mono), "monospace");
        let id = store.create("x 日本語", TextStyle::new(13.0).family(mono), None);
        let width = store.size(id).width;
        assert!(width.is_finite() && width > 0.0, "{width}");
    }

    #[test]
    fn monospace_stands_for_the_first_loaded_candidate() {
        let mut db = fontdb::Database::new();
        db.load_font_data(BUNDLED_FONT.to_vec());
        assert_eq!(
            first_loaded(&db, &["No Such Mono", BUNDLED_FAMILY]),
            Some(BUNDLED_FAMILY)
        );
        assert_eq!(first_loaded(&db, &["No Such Mono"]), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn monospace_is_a_mac_terminal_font_on_macos() {
        let mut store = TextStore::new(FontOptions::default());
        let mono = store.font_family("monospace");
        let name = store.family_name(mono);
        assert!(MONOSPACE_FAMILIES.contains(&name), "{name}");
    }

    /// A glyph: its bytes, glyph id, position and advance.
    type Glyph = (usize, usize, u16, f32, f32, f32);
    /// A visual line: paragraph, top, height, width, and its glyphs.
    type Run = (usize, f32, f32, f32, Vec<Glyph>);

    fn runs(buffer: &Buffer) -> Vec<Run> {
        buffer
            .layout_runs()
            .map(|run| {
                let glyphs = run
                    .glyphs
                    .iter()
                    .map(|g| (g.start, g.end, g.glyph_id, g.x, g.y, g.w))
                    .collect();
                (
                    run.line_i,
                    run.line_top,
                    run.line_height,
                    run.line_w,
                    glyphs,
                )
            })
            .collect()
    }

    #[test]
    fn layouts_match_shaping_into_a_buffer_made_with_buffer_new() {
        // `create` starts from an empty buffer instead of `Buffer::new` (which shapes an
        // empty text first): the result must be the same glyphs in the same places.
        let mut store = store();
        let cases: [(&str, TextStyle, Option<f32>); 8] = [
            ("", TextStyle::new(14.0), None),
            ("Hello, world", TextStyle::new(14.0), None),
            ("ffi fl office", TextStyle::new(20.0).weight(700), None),
            (
                "one two three four five six",
                TextStyle::new(16.0),
                Some(60.0),
            ),
            ("ab\ncd\r\nef\n", TextStyle::new(13.0).italic(true), None),
            ("x 日本語 العربية", TextStyle::new(13.0), Some(40.0)),
            (
                "   leading and trailing   ",
                TextStyle::new(12.0),
                Some(1.0),
            ),
            ("tab\tstop", TextStyle::new(15.0), None),
        ];
        for (text, style, wrap) in cases {
            let id = store.create(text, style, wrap);
            let (font_system, layouts) = store.split_mut();
            let mut reference = Buffer::new(
                font_system,
                Metrics::new(style.font_size, style.line_height),
            );
            reference.set_size(wrap, None);
            let attrs = Attrs::new()
                .family(Family::SansSerif)
                .weight(Weight(style.weight))
                .style(if style.italic {
                    Style::Italic
                } else {
                    Style::Normal
                });
            reference.set_text(text, &attrs, Shaping::Advanced, None);
            reference.shape_until_scroll(font_system, false);
            assert_eq!(runs(layouts.buffer(id)), runs(&reference), "{text:?}");
            assert_eq!(layouts.size(id), measure(&reference), "{text:?}");
        }
    }

    #[test]
    fn a_text_made_again_is_laid_out_as_a_fresh_store_lays_it_out() {
        let mut store = store();
        let cases: [(&str, TextStyle, Option<f32>); 9] = [
            ("", TextStyle::new(14.0), None),
            ("Document", TextStyle::new(13.0), None),
            ("ffi fl office", TextStyle::new(20.0).weight(700), None),
            (
                "one two three four five six",
                TextStyle::new(16.0),
                Some(60.0),
            ),
            ("ab\ncd\r\nef\n", TextStyle::new(13.0).italic(true), None),
            ("x 日本語 العربية", TextStyle::new(13.0), Some(40.0)),
            (
                "centered\nlines",
                TextStyle::new(13.0).align(TextAlign::Center),
                None,
            ),
            (
                "end",
                TextStyle::new(13.0).align(TextAlign::End),
                Some(90.0),
            ),
            ("tab\tstop", TextStyle::new(15.0), None),
        ];
        for (text, style, wrap) in cases {
            let first = store.create(text, style, wrap);
            let again = store.create(text, style, wrap);
            assert_ne!(first, again, "a layout of its own");
            let mut fresh = self::store();
            let reference = fresh.create(text, style, wrap);
            assert_eq!(placed(&store, again), placed(&fresh, reference), "{text:?}");
            assert_eq!(
                store.layouts().buffer(again).lines.len(),
                fresh.layouts().buffer(reference).lines.len()
            );
        }
    }

    #[test]
    fn only_the_same_text_style_and_width_is_copied() {
        let mut store = store();
        let base = TextStyle::new(14.0);
        let original = store.create("Wide text here", base, None);
        let variants: [(&str, TextStyle, Option<f32>); 8] = [
            ("Wide text her", base, None),
            ("Wide text here\n", base, None),
            ("Wide text here", base, Some(50.0)),
            ("Wide text here", TextStyle::new(15.0), None),
            ("Wide text here", base.weight(700), None),
            ("Wide text here", base.italic(true), None),
            ("Wide text here", base.align(TextAlign::Center), Some(50.0)),
            (
                "Wide text here",
                TextStyle {
                    line_height: 30.0,
                    ..base
                },
                None,
            ),
        ];
        for (text, style, wrap) in variants {
            let id = store.create(text, style, wrap);
            let mut fresh = self::store();
            let reference = fresh.create(text, style, wrap);
            assert_eq!(
                placed(&store, id),
                placed(&fresh, reference),
                "{text:?} {style:?} {wrap:?}"
            );
        }
        let mut fresh = self::store();
        let reference = fresh.create("Wide text here", base, None);
        assert_eq!(placed(&store, original), placed(&fresh, reference));
    }

    #[test]
    fn copies_outlive_and_ignore_changes_to_the_layout_they_came_from() {
        let mut store = store();
        let style = TextStyle::new(16.0);
        let text = "one two three four five six";
        let mut fresh = self::store();
        let whole = fresh.create(text, style, None);
        let narrow = fresh.create(text, style, Some(60.0));

        let original = store.create(text, style, None);
        let copy = store.create(text, style, None);
        assert!(store.rewrap(original, style, Some(60.0)));
        assert_eq!(placed(&store, original), placed(&fresh, narrow));
        assert_eq!(placed(&store, copy), placed(&fresh, whole));
        let after_rewrap = store.create(text, style, None);
        assert_eq!(placed(&store, after_rewrap), placed(&fresh, whole));
        store.remove(original);
        store.remove(after_rewrap);
        let other = store.create("other", style, None);
        assert_eq!(other, after_rewrap, "the freed slot is reused");
        let later = store.create(text, style, None);
        assert_eq!(placed(&store, later), placed(&fresh, whole));
        assert_eq!(placed(&store, copy), placed(&fresh, whole));
        let other_reference = fresh.create("other", style, None);
        assert_eq!(placed(&store, other), placed(&fresh, other_reference));
    }

    #[test]
    fn many_repeated_texts_are_each_laid_out_as_a_fresh_store_lays_them_out() {
        let mut store = store();
        let mut fresh = self::store();
        let style = TextStyle::new(13.0);
        let widths = [None, Some(40.0), Some(-5.0), Some(0.0)];
        let mut kept = Vec::new();
        for i in 0..600_usize {
            let text = format!("cell {} of {}", i % 37, i % 5);
            let wrap = widths[i % widths.len()];
            let id = store.create(&text, style, wrap);
            let reference = fresh.create(&text, style, wrap);
            assert_eq!(
                placed(&store, id),
                placed(&fresh, reference),
                "{text} {wrap:?}"
            );
            fresh.remove(reference);
            match i % 7 {
                0 => store.remove(id),
                1 => {
                    let rewrap = widths[(i + 1) % widths.len()];
                    assert!(store.rewrap(id, style, rewrap));
                    kept.push((id, text, rewrap));
                }
                _ => kept.push((id, text, wrap)),
            }
        }
        for (id, text, wrap) in kept {
            let reference = fresh.create(&text, style, wrap);
            assert_eq!(
                placed(&store, id),
                placed(&fresh, reference),
                "{text} {wrap:?}"
            );
            fresh.remove(reference);
        }
    }

    #[test]
    fn removed_ids_are_reused() {
        let mut store = store();
        let a = store.create("a", TextStyle::new(12.0), None);
        let _b = store.create("b", TextStyle::new(12.0), None);
        store.remove(a);
        assert_eq!(store.layouts().len(), 1);
        let c = store.create("c", TextStyle::new(12.0), None);
        assert_eq!(c, a);
    }
}

#[cfg(test)]
mod align_tests {
    use super::*;

    fn store() -> TextStore {
        TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        })
    }

    /// (left edge of the first glyph, line width) of each line.
    fn lines(store: &TextStore, id: TextLayoutId) -> Vec<(f32, f32)> {
        store
            .layouts()
            .buffer(id)
            .layout_runs()
            .map(|run| (run.glyphs.first().map_or(0.0, |g| g.x), run.line_w))
            .collect()
    }

    #[test]
    fn centered_and_end_aligned_lines_line_up_inside_the_widest_one() {
        let mut store = store();
        let text = "Hi\nA much longer second line";
        let start = store.create(text, TextStyle::new(14.0), None);
        let center = store.create(text, TextStyle::new(14.0).align(TextAlign::Center), None);
        let end = store.create(text, TextStyle::new(14.0).align(TextAlign::End), None);
        // The block keeps the size of the unaligned text.
        assert_eq!(store.size(center), store.size(start));
        assert_eq!(store.size(end), store.size(start));
        let width = store.size(start).width;
        let [(x0, w0), (x1, _)] = lines(&store, start)[..] else {
            panic!("two lines")
        };
        assert_eq!((x0, x1), (0.0, 0.0));
        let [(c0, _), (c1, cw1)] = lines(&store, center)[..] else {
            panic!("two lines")
        };
        assert!((c0 - (width - w0) / 2.0).abs() < 1.0, "{c0} in {width}");
        assert!(
            c1.abs() < 1.0 && (width - cw1).abs() < 1.0,
            "the widest line stays put"
        );
        let [(e0, _), _] = lines(&store, end)[..] else {
            panic!("two lines")
        };
        assert!((e0 - (width - w0)).abs() < 1.0, "{e0} in {width}");
    }

    #[test]
    fn aligning_wrapped_text_keeps_its_lines() {
        let mut store = store();
        let text = "The quick brown fox jumps over the lazy dog, twice over";
        let plain = store.create(text, TextStyle::new(14.0), Some(120.0));
        let center = store.create(
            text,
            TextStyle::new(14.0).align(TextAlign::Center),
            Some(120.0),
        );
        assert_eq!(lines(&store, plain).len(), lines(&store, center).len());
        assert_eq!(store.size(plain), store.size(center));
        assert!(store.size(center).width <= 120.0);
        assert!(lines(&store, center).iter().any(|&(x, _)| x > 1.0));
    }
}

#[cfg(test)]
mod cluster_tests {
    use super::*;

    fn store() -> TextStore {
        TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        })
    }

    #[test]
    fn clusters_cover_their_bytes_across_paragraphs_and_lines() {
        let mut store = store();
        let style = TextStyle::new(14.0);
        let id = store.create("ab\ncdé", style, None);
        let clusters = store.clusters(id, 0..usize::MAX);
        let bytes: Vec<_> = clusters.iter().map(|c| c.0.clone()).collect();
        assert_eq!(bytes, [0..1, 1..2, 3..4, 4..5, 5..7]);
        // Each sits where the caret before it is, on its own line.
        for (range, rect) in &clusters {
            let caret = store.caret(id, range.start);
            assert_eq!((rect.x, rect.y), (caret.x, caret.top), "{range:?}");
            assert_eq!(rect.height, style.line_height);
        }
        assert_eq!(clusters[2].1.y, style.line_height);
        // Only those overlapping the range asked for.
        let middle: Vec<_> = store.clusters(id, 4..6).into_iter().map(|c| c.0).collect();
        assert_eq!(middle, [4..5, 5..7]);
    }
}
