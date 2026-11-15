use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::ops::Range;

use noa_gpui_scene::{Point, Rect, Size, TextLayoutId};
use noa_gpui_text::{TextStore, TextStyle};

use super::CachedText;
use super::label::{CandidateSizes, Truncation, fit_measured_line_kept};
use super::scroller::Scroller;
use crate::event::{Key, Modifiers};
use crate::widget::EventCx;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    None,
    Single,
    #[default]
    Multiple,
}

/// Emitted when the selected rows or items of a table, outline, collection or browser
/// change. Read the selection from the widget (it may be large).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowSelectionChanged {
    /// The row or item the user acted on last (the keyboard focus within the view).
    pub lead: Option<usize>,
    pub count: usize,
}

/// Selected ids (rows of the data, independent of the order they are shown in).
///
/// Positions are indices into the displayed order, passed in as `order` (`order[pos]`
/// is the id shown at `pos`). Selecting everything or a range touches only those ids,
/// so it stays cheap with 100,000 rows.
#[derive(Debug, Clone, Default)]
pub(crate) struct Selection {
    pub mode: SelectionMode,
    marks: Vec<bool>,
    count: usize,
    /// Every marked id is in `touched.0..touched.1` (empty: none is), so clearing a
    /// selection of a few rows of 100,000 does not go through them all.
    touched: (usize, usize),
    /// Where a Shift range starts (an id).
    anchor: Option<usize>,
    /// The row acted on last (an id).
    pub lead: Option<usize>,
    /// Where `anchor` and `lead` were last seen in the displayed order. Only hints: the
    /// order may have changed since, so they are checked before use (see `position`).
    anchor_at: usize,
    lead_at: usize,
}

impl Selection {
    pub fn new(mode: SelectionMode, len: usize) -> Self {
        Self {
            mode,
            marks: vec![false; len],
            ..Default::default()
        }
    }

    /// Grow or shrink to `len` ids, dropping selected ids past the end.
    pub fn resize(&mut self, len: usize) {
        if len < self.marks.len() {
            let (start, end) = self.touched;
            let gone = start.max(len)..end.max(len);
            self.count -= self.marks[gone].iter().filter(|&&m| m).count();
            self.touched = (start.min(len), end.min(len));
        }
        self.marks.resize(len, false);
        if self.lead.is_some_and(|id| id >= len) {
            self.lead = None;
        }
        if self.anchor.is_some_and(|id| id >= len) {
            self.anchor = None;
        }
    }

    pub fn is_selected(&self, id: usize) -> bool {
        self.marks.get(id).copied().unwrap_or(false)
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// Selected ids in ascending order.
    pub fn ids(&self) -> Vec<usize> {
        if self.count == 0 {
            return Vec::new();
        }
        let (start, end) = self.touched;
        (start..end).filter(|&id| self.marks[id]).collect()
    }

    pub fn message(&self) -> RowSelectionChanged {
        RowSelectionChanged {
            lead: self.lead,
            count: self.count,
        }
    }

    fn mark(&mut self, id: usize, on: bool) {
        if let Some(m) = self.marks.get_mut(id)
            && *m != on
        {
            *m = on;
            if on {
                self.count += 1;
                self.touch(id..id + 1);
            } else {
                self.count -= 1;
            }
        }
    }

    /// Mark `ids`, widening the touched span once rather than for each id: to just
    /// those ids when there are a few, to every id otherwise.
    fn mark_all(&mut self, ids: &[usize]) {
        let len = self.marks.len();
        let valid = ids.iter().copied().filter(|&id| id < len);
        match (ids.len() <= 64).then(|| valid.clone().min().zip(valid.max())) {
            Some(None) => return,
            Some(Some((min, max))) => self.touch(min..max + 1),
            None => self.touch(0..len),
        }
        for &id in ids {
            if let Some(m) = self.marks.get_mut(id)
                && !*m
            {
                *m = true;
                self.count += 1;
            }
        }
    }

    fn touch(&mut self, range: Range<usize>) {
        let (start, end) = self.touched;
        self.touched = if start < end {
            (start.min(range.start), end.max(range.end))
        } else {
            (range.start, range.end)
        };
    }

    /// The ids were renumbered (a hierarchy changed shape): `map[old]` is the new id of
    /// old id `old`, `None` when it is gone. There are `len` ids now.
    pub fn remap(&mut self, len: usize, map: &[Option<usize>]) {
        let new = |id: usize| map.get(id).copied().flatten();
        let ids: Vec<usize> = self.ids().into_iter().filter_map(new).collect();
        let (anchor, lead) = (self.anchor.and_then(new), self.lead.and_then(new));
        self.marks = vec![false; len];
        self.count = 0;
        self.touched = (0, 0);
        self.mark_all(&ids);
        self.anchor = anchor;
        self.lead = lead;
    }

    /// Unselect `id` (the lead and anchor stay).
    pub fn unselect(&mut self, id: usize) {
        self.mark(id, false);
    }

    /// Unselect the ids `keep` refuses, and forget an anchor or lead it refuses.
    pub fn retain(&mut self, keep: impl Fn(usize) -> bool) {
        if self.count > 0 {
            for id in self.touched.0..self.touched.1 {
                if self.marks[id] && !keep(id) {
                    self.mark(id, false);
                }
            }
        }
        if self.anchor.is_some_and(|id| !keep(id)) {
            self.anchor = None;
        }
        if self.lead.is_some_and(|id| !keep(id)) {
            self.lead = None;
        }
    }

    pub fn clear(&mut self) {
        if self.count > 0 {
            let (start, end) = self.touched;
            self.marks[start..end].fill(false);
            self.count = 0;
        }
        self.touched = (0, 0);
    }

    /// Select only `id` (or nothing), making it the anchor and lead.
    pub fn set(&mut self, id: Option<usize>) {
        if self.mode == SelectionMode::None {
            return;
        }
        let id = id.filter(|&id| id < self.marks.len());
        self.clear();
        if let Some(id) = id {
            self.mark(id, true);
        }
        self.anchor = id;
        self.lead = id;
    }

    /// Select `ids` (in Multiple mode; only the last one otherwise).
    pub fn set_many(&mut self, ids: impl IntoIterator<Item = usize>) {
        let ids: Vec<usize> = ids
            .into_iter()
            .filter(|&id| id < self.marks.len())
            .collect();
        if self.mode != SelectionMode::Multiple {
            self.set(ids.last().copied());
            return;
        }
        self.clear();
        for &id in &ids {
            self.mark(id, true);
        }
        self.anchor = ids.first().copied();
        self.lead = ids.last().copied();
    }

    /// The displayed position of `id`. `order` holds each id once, so a `hint` that
    /// still points at `id` is its position; otherwise the order is searched.
    fn position(order: &[usize], id: Option<usize>, hint: usize) -> Option<usize> {
        let id = id?;
        if order.get(hint) == Some(&id) {
            return Some(hint);
        }
        order.iter().position(|&o| o == id)
    }

    pub fn lead_position(&self, order: &[usize]) -> Option<usize> {
        Self::position(order, self.lead, self.lead_at)
    }

    fn shown_at(&mut self, pos: usize) {
        self.anchor_at = pos;
        self.lead_at = pos;
    }

    fn select_range(&mut self, order: &[usize], a: usize, b: usize) {
        self.clear();
        let (lo, hi) = (a.min(b), a.max(b));
        self.mark_all(&order[lo..=hi.min(order.len() - 1)]);
    }

    /// A click on position `pos`. Returns whether the selection changed.
    pub fn click(&mut self, order: &[usize], pos: usize, modifiers: Modifiers) -> bool {
        let Some(&id) = order.get(pos) else {
            return false;
        };
        match self.mode {
            SelectionMode::None => false,
            SelectionMode::Single => {
                let changed = !self.is_selected(id) || self.count != 1;
                self.set(Some(id));
                self.shown_at(pos);
                changed
            }
            SelectionMode::Multiple => {
                if modifiers.command {
                    let on = !self.is_selected(id);
                    self.mark(id, on);
                    self.anchor = Some(id);
                    self.lead = Some(id);
                    self.shown_at(pos);
                } else if modifiers.shift
                    && let Some(anchor) = Self::position(order, self.anchor, self.anchor_at)
                {
                    self.select_range(order, anchor, pos);
                    self.lead = Some(id);
                    self.anchor_at = anchor;
                    self.lead_at = pos;
                } else {
                    if self.count == 1 && self.is_selected(id) {
                        self.anchor = Some(id);
                        self.lead = Some(id);
                        self.shown_at(pos);
                        return false;
                    }
                    self.set(Some(id));
                    self.shown_at(pos);
                }
                true
            }
        }
    }

    /// Move the lead to `pos`; `extend` (Shift) grows the range from the anchor.
    pub fn move_to(&mut self, order: &[usize], pos: usize, extend: bool) -> bool {
        let Some(&id) = order.get(pos) else {
            return false;
        };
        if self.mode == SelectionMode::None {
            return false;
        }
        if extend
            && self.mode == SelectionMode::Multiple
            && let Some(anchor) = Self::position(order, self.anchor, self.anchor_at)
        {
            self.select_range(order, anchor, pos);
            self.lead = Some(id);
            self.anchor_at = anchor;
            self.lead_at = pos;
        } else {
            if self.count == 1 && self.is_selected(id) {
                return false;
            }
            self.set(Some(id));
            self.shown_at(pos);
        }
        true
    }

    /// Cmd+A. Returns whether anything changed.
    pub fn select_all(&mut self, order: &[usize]) -> bool {
        if self.mode != SelectionMode::Multiple || self.count == order.len() {
            return false;
        }
        self.mark_all(order);
        if self.anchor.is_none() {
            self.anchor = order.first().copied();
            self.lead = self.anchor;
            self.shown_at(0);
        }
        true
    }
}

const EDGE_SCROLL_EDGE: f32 = 24.0;
/// How fast a drag at (or past) the edge scrolls, in pixels per second.
const EDGE_SCROLL_SPEED: f32 = 900.0;

/// Scrolling a list while items are dragged at (or past) its top or bottom: faster the
/// further in (or out) the pointer is, and only while there is more to scroll to.
#[derive(Debug, Default)]
pub(crate) struct EdgeScroll {
    /// Where the pointer is (widget coordinates) and when the list last scrolled.
    at: Option<(Point, f64)>,
}

impl EdgeScroll {
    /// How fast (-1 up to 1 down) a drag at `p` scrolls the list shown in `viewport`.
    fn rate(viewport: Rect, scroller: &Scroller, p: Point) -> f32 {
        let edge = EDGE_SCROLL_EDGE.min(viewport.height / 4.0);
        if edge <= 0.0 {
            return 0.0;
        }
        let up = (viewport.y + edge - p.y) / edge;
        let down = (p.y - (viewport.bottom() - edge)) / edge;
        if up > 0.0 && scroller.offset.y > 0.0 {
            -up.min(1.0)
        } else if down > 0.0 && scroller.offset.y < scroller.max().y {
            down.min(1.0)
        } else {
            0.0
        }
    }

    pub fn track(&mut self, cx: &mut EventCx, viewport: Rect, scroller: &Scroller, p: Point) {
        if Self::rate(viewport, scroller, p) == 0.0 {
            self.at = None;
            return;
        }
        let since = self.at.map_or(cx.now(), |(_, t)| t);
        if self.at.is_none() {
            cx.request_animation_frame();
        }
        self.at = Some((p, since));
    }

    /// One animation frame at `now`: scroll, and return where the pointer is if the list
    /// moved under it (the drop place follows).
    pub fn step(
        &mut self,
        cx: &mut EventCx,
        viewport: Rect,
        scroller: &mut Scroller,
        now: f64,
    ) -> Option<Point> {
        let (p, last) = self.at?;
        let rate = Self::rate(viewport, scroller, p);
        // Long frames (or the first one) move at most a 30th of a second's worth.
        let dt = (now - last).clamp(0.0, 1.0 / 30.0) as f32;
        let offset = scroller.offset;
        let target = Point::new(
            offset.x,
            offset.y + rate * EDGE_SCROLL_SPEED * dt.max(1.0 / 120.0),
        );
        if rate == 0.0 || !scroller.set_offset(target) {
            self.at = None;
            return None;
        }
        self.at = Some((p, now));
        cx.request_paint();
        cx.request_animation_frame();
        Some(p)
    }

    pub fn stop(&mut self) {
        self.at = None;
    }
}

/// The group row pinned to the top of a scrolled list and where it is drawn: the last
/// of the group rows at displayed positions `groups` (ascending) that has scrolled
/// above the top, pushed up by the next one as it arrives. Rows are `row_height` tall,
/// scrolled by `offset`, and the list's top is at `top` (widget `y`).
pub(crate) fn pinned_group(
    groups: &[usize],
    offset: f32,
    row_height: f32,
    top: f32,
) -> Option<(usize, f32)> {
    let first = (offset.max(0.0) / row_height) as usize;
    let i = groups.partition_point(|&g| g <= first);
    let pos = *groups.get(i.checked_sub(1)?)?;
    if pos as f32 * row_height >= offset {
        return None;
    }
    let y = match groups.get(i) {
        Some(&next) => top.min(top + next as f32 * row_height - offset - row_height),
        None => top,
    };
    Some((pos, y))
}

/// The part of a list (in content coordinates, `x` and `width` as given) to scroll
/// into view to show the row at `pos`: the row, and above it room for the group row
/// that would be pinned over it, if any of `groups` (ascending displayed positions)
/// heads it and `floats`.
pub(crate) fn reveal_rect(
    groups: &[usize],
    floats: bool,
    pos: usize,
    row_height: f32,
    x: f32,
    width: f32,
) -> Rect {
    let y = pos as f32 * row_height;
    let pinned = floats && groups.first().is_some_and(|&g| g < pos);
    if pinned {
        Rect::new(x, y - row_height, width, 2.0 * row_height)
    } else {
        Rect::new(x, y, width, row_height)
    }
}

/// Where a vertical navigation key moves the lead among `len` rows, from `current`
/// (`None`: nothing selected yet). `page` is how many rows fit in the view.
pub(crate) fn step(key: Key, current: Option<usize>, len: usize, page: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let page = page.max(1);
    Some(match (key, current) {
        (Key::ArrowUp, None) => last,
        (Key::ArrowDown, None) => 0,
        (Key::ArrowUp, Some(c)) => c.saturating_sub(1),
        (Key::ArrowDown, Some(c)) => (c + 1).min(last),
        (Key::Home, _) => 0,
        (Key::End, _) => last,
        (Key::PageUp, c) => c.unwrap_or(0).saturating_sub(page),
        (Key::PageDown, c) => (c.unwrap_or(0) + page).min(last),
        _ => return None,
    })
}

/// Text layouts for the cells drawn by the last paint. Call [`TextCache::begin`] at the
/// start of a paint, [`TextCache::get`] for each visible cell, then
/// [`TextCache::end`]: layouts of cells that scrolled away are released.
#[derive(Debug)]
pub(crate) struct TextCache<K> {
    live: CellMap<K>,
    previous: CellMap<K>,
}

/// Cells by their key: a few small integers (positions, ids, columns), looked up two or
/// three times per cell drawn. Hashed with a multiplication per integer, not SipHash.
type CellMap<K> = HashMap<K, Entry, BuildHasherDefault<CellHasher>>;

/// Hashes the integers of a cell key (made from positions and ids, never from what
/// the user types) by mixing each in with one multiplication.
#[derive(Default)]
struct CellHasher(u64);

impl Hasher for CellHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }

    fn finish(&self) -> u64 {
        // The map takes buckets from the low bits and tags from the high ones: give
        // the low bits the well-mixed high half too.
        self.0 ^ (self.0 >> 32)
    }
}

#[derive(Debug, Default)]
struct Entry {
    text: CachedText,
    fit: Option<Fit>,
}

/// The text shortened for one width: kept so that repainting an unchanged cell does not
/// measure it again.
#[derive(Debug)]
struct Fit {
    source: String,
    room: u32,
    truncation: Truncation,
    /// Boxed: most cells are drawn whole, and every cell drawn is moved in and out of
    /// the cache each paint.
    shortened: Option<Box<Shortened>>,
}

#[derive(Debug)]
struct Shortened {
    shown: String,
    /// The whole text's width on one line in `style`: the same at every room, so a
    /// resized column does not shape the whole text again.
    full_width: f32,
    style: TextStyle,
    sizes: CandidateSizes,
}

impl<K> Default for TextCache<K> {
    fn default() -> Self {
        Self {
            live: CellMap::default(),
            previous: CellMap::default(),
        }
    }
}

impl<K: Hash + Eq + Copy> TextCache<K> {
    pub fn begin(&mut self) {
        std::mem::swap(&mut self.live, &mut self.previous);
    }

    pub fn get(
        &mut self,
        store: &mut TextStore,
        key: K,
        text: &str,
        style: TextStyle,
    ) -> (Option<TextLayoutId>, Size) {
        let (layout, size, _) = self.get_fitted(store, key, text, style, None);
        (layout, size)
    }

    /// Like [`TextCache::get`], but shortened with an ellipsis to fit `fit`'s width
    /// (`(room, truncation)`); also says whether the text was shortened.
    pub fn get_fitted(
        &mut self,
        store: &mut TextStore,
        key: K,
        text: &str,
        style: TextStyle,
        fit: Option<(f32, Truncation)>,
    ) -> (Option<TextLayoutId>, Size, bool) {
        let Self { live, previous } = self;
        let entry = live
            .entry(key)
            .or_insert_with(|| previous.remove(&key).unwrap_or_default());
        let fit = fit.filter(|(_, truncation)| *truncation != Truncation::None);
        match fit {
            Some((room, truncation)) => {
                // Whole pixels: a column dragged a fraction wider shows the same text.
                let room_px = room.max(0.0).floor() as u32;
                let fresh = entry.fit.as_ref().is_some_and(|f| {
                    f.source == text && f.room == room_px && f.truncation == truncation
                });
                if !fresh {
                    let known = entry.fit.take().and_then(|f| {
                        let s = *f.shortened?;
                        (f.source == text && s.style == style).then(|| {
                            let sizes = (f.truncation == truncation).then_some(s.sizes);
                            (s.full_width, sizes.unwrap_or_default())
                        })
                    });
                    // Otherwise shape the whole text first: it is what is drawn when it
                    // fits (most cells), and its width says whether it does. (The cell
                    // holds it already when it was drawn whole.)
                    let (full, mut sizes) = match known {
                        Some(known) => known,
                        None => (
                            entry.text.shape(store, text, style, None).width,
                            CandidateSizes::default(),
                        ),
                    };
                    let room = room_px as f32;
                    let shortened = fit_measured_line_kept(
                        store, text, full, style, room, truncation, &mut sizes,
                    )
                    .map(|(shown, layout)| {
                        // The search shaped the shortened text: draw that layout
                        // rather than shape it again.
                        if let Some(id) = layout {
                            entry.text.adopt(store, id, &shown, None);
                        }
                        Box::new(Shortened {
                            shown,
                            full_width: full,
                            style,
                            sizes,
                        })
                    });
                    entry.fit = Some(Fit {
                        source: text.to_owned(),
                        room: room_px,
                        truncation,
                        shortened,
                    });
                }
            }
            None => entry.fit = None,
        }
        let shown = entry.fit.as_ref().and_then(|f| f.shortened.as_deref());
        let shown = shown.map(|s| s.shown.as_str());
        let size = entry.text.shape(store, shown.unwrap_or(text), style, None);
        let shortened = shown.is_some();
        let layout = entry.text.layout();
        (layout, size, shortened)
    }

    pub fn end(&mut self, store: &mut TextStore) {
        for (_, mut entry) in self.previous.drain() {
            entry.text.release(store);
        }
    }

    pub fn release(&mut self, store: &mut TextStore) {
        self.begin();
        self.end(store);
        self.begin();
        self.end(store);
    }
}

#[cfg(test)]
mod tests {
    use super::{CellHasher, Selection, SelectionMode, pinned_group};
    use std::hash::{BuildHasher, BuildHasherDefault};

    #[test]
    fn invalid_selection_ids_cannot_become_the_anchor_or_lead() {
        for mode in [SelectionMode::Single, SelectionMode::Multiple] {
            let mut selection = Selection::new(mode, 3);
            selection.set_many([usize::MAX, 1, usize::MAX]);
            assert_eq!(selection.ids(), [1]);
            assert_eq!(selection.anchor, Some(1));
            assert_eq!(selection.lead, Some(1));

            selection.set_many([3, usize::MAX]);
            assert!(selection.ids().is_empty());
            assert_eq!(selection.count(), 0);
            assert_eq!(selection.anchor, None);
            assert_eq!(selection.lead, None);

            selection.set(Some(usize::MAX));
            assert_eq!(selection.anchor, None);
            assert_eq!(selection.lead, None);
        }
    }

    #[test]
    fn cell_keys_hash_apart_in_both_halves() {
        // Rows, columns and lines as the table uses them: no two may share a hash, and
        // neither the low bits (buckets) nor the top 7 (tags) may collapse.
        let hasher = BuildHasherDefault::<CellHasher>::default();
        let mut hashes = Vec::new();
        for row in 0..2000_usize {
            for column in 0..8_usize {
                for line in 0..2_usize {
                    hashes.push(hasher.hash_one((row, column, line)));
                }
            }
        }
        let count = hashes.len();
        let distinct = |f: &dyn Fn(u64) -> u64| {
            let mut v: Vec<u64> = hashes.iter().map(|&h| f(h)).collect();
            v.sort_unstable();
            v.dedup();
            v.len()
        };
        assert_eq!(distinct(&|h| h), count);
        assert_eq!(
            distinct(&|h| h & 0xfff),
            1 << 12,
            "low bits fill every bucket"
        );
        assert_eq!(distinct(&|h| h >> 57), 128, "every tag is used");
        let ids: Vec<u64> = (0..4096_usize).map(|id| hasher.hash_one(id)).collect();
        assert!(
            ids.iter()
                .map(|h| h & 0xfff)
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 2500
        );
    }

    fn model(len: usize, ids: &[usize]) -> Vec<bool> {
        let mut flags = vec![false; len];
        for &id in ids {
            flags[id] = true;
        }
        flags
    }

    fn flags(selection: &Selection, len: usize) -> Vec<bool> {
        (0..len + 70).map(|id| selection.is_selected(id)).collect()
    }

    #[test]
    fn selections_keep_their_ids_across_word_edges_while_resized_and_filtered() {
        let picked = [0, 1, 62, 63, 64, 65, 127, 128, 129, 190, 191, 199];
        for len in [0, 1, 63, 64, 65, 128, 129, 200] {
            let ids: Vec<usize> = picked.iter().copied().filter(|&id| id < len).collect();
            let mut selection = Selection::new(SelectionMode::Multiple, len);
            selection.set_many(ids.iter().copied().chain([len, len + 64]));
            assert_eq!(selection.count(), ids.len(), "ids past the end are ignored");
            assert_eq!(selection.ids(), ids);
            let mut expected = model(len, &ids);
            expected.resize(len + 70, false);
            assert_eq!(flags(&selection, len), expected);

            for shorter in [len, len.saturating_sub(1), len / 2, 64, 63, 1, 0] {
                let shorter = shorter.min(len);
                let mut cut = selection.clone();
                cut.resize(shorter);
                let kept: Vec<usize> = ids.iter().copied().filter(|&id| id < shorter).collect();
                assert_eq!(cut.ids(), kept, "{len} -> {shorter}");
                assert_eq!(cut.count(), kept.len());
                // Growing again brings nothing back.
                cut.resize(len + 64);
                assert_eq!(cut.ids(), kept);
                assert!(!cut.is_selected(len + 1));
            }

            let mut even = selection.clone();
            let asked = std::cell::RefCell::new(Vec::new());
            let keep = |id: usize| id.is_multiple_of(2);
            even.retain(|id| {
                asked.borrow_mut().push(id);
                keep(id)
            });
            let asked = asked.into_inner();
            assert_eq!(asked[..ids.len()], ids, "about each selected id, in order");
            let kept: Vec<usize> = ids.iter().copied().filter(|&id| keep(id)).collect();
            assert_eq!(even.ids(), kept);
            assert_eq!(even.count(), kept.len());

            let shown: Vec<usize> = (0..len).rev().filter(|id| id % 3 != 1).collect();
            let mut all = selection.clone();
            all.select_all(&shown);
            let mut union = model(len, &ids);
            for &id in &shown {
                union[id] = true;
            }
            assert_eq!(all.count(), union.iter().filter(|&&f| f).count());
            union.resize(len + 70, false);
            assert_eq!(flags(&all, len), union);
            if shown.len() > 2 {
                let mut range = selection.clone();
                range.click(&shown, 1, Default::default());
                let shift = crate::event::Modifiers {
                    shift: true,
                    ..Default::default()
                };
                range.click(&shown, shown.len() - 1, shift);
                let mut expected = shown[1..].to_vec();
                expected.sort_unstable();
                assert_eq!(range.ids(), expected);
                assert_eq!(range.count(), expected.len());
            }

            selection.clear();
            assert_eq!((selection.count(), selection.ids()), (0, vec![]));
            assert_eq!(flags(&selection, len), vec![false; len + 70]);
        }
    }

    #[test]
    fn the_group_scrolled_past_is_pinned_until_the_next_one_pushes_it_up() {
        // Group rows at positions 0 and 5, rows 20 tall, the list's top at y = 30.
        let groups = [0, 5];
        let at = |offset| pinned_group(&groups, offset, 20.0, 30.0);
        assert_eq!(at(0.0), None, "in its place");
        assert_eq!(at(-8.0), None, "pulled down past the top");
        assert_eq!(at(10.0), Some((0, 30.0)));
        assert_eq!(at(80.0), Some((0, 30.0)));
        // Group 5 (at 100) is 10 below the top: group 0 is pushed up by 10.
        assert_eq!(at(90.0), Some((0, 20.0)));
        assert_eq!(at(100.0), None, "group 5 is in its place at the top");
        assert_eq!(at(130.0), Some((5, 30.0)));
        assert_eq!(pinned_group(&[3], 20.0, 20.0, 0.0), None, "no group above");
    }
}

/// A search typed into a field: text matches when it holds each word, in any letter
/// case.
#[derive(Debug, Clone, Default)]
pub(crate) struct TextQuery {
    text: String,
    words: Vec<String>,
}

impl TextQuery {
    /// Take `text` as the query. Returns whether it changed.
    pub fn set(&mut self, text: &str) -> bool {
        if text == self.text {
            return false;
        }
        text.clone_into(&mut self.text);
        self.words = text.split_whitespace().map(str::to_lowercase).collect();
        true
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// Whether there are no words: everything matches.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn matches(&self, text: &str) -> bool {
        self.words.iter().all(|w| contains_ignoring_case(text, w))
    }

    /// Byte ranges of `text` where a word is, in any letter case, in order; words that
    /// overlap make one range.
    pub fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        let mut found: Vec<Range<usize>> = self
            .words
            .iter()
            .flat_map(|word| find_ignoring_case(text, word))
            .collect();
        found.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(found.len());
        for r in found {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        merged
    }
}

/// Whether `text` holds `word` (lowercase) in any letter case. ASCII text is compared
/// in place; other text is lowercased first.
pub(crate) fn contains_ignoring_case(text: &str, word: &str) -> bool {
    if text.is_ascii() {
        let (text, word) = (text.as_bytes(), word.as_bytes());
        let Some((&first, rest)) = word.split_first() else {
            return true;
        };
        let Some(last_start) = text.len().checked_sub(word.len()) else {
            return false;
        };
        // `word` is lowercase already: only the text's letters are lowercased, and a
        // start is first looked for as the first letter in either case.
        let first_upper = first.to_ascii_uppercase();
        let after_first = &text[1..];
        text[..=last_start].iter().enumerate().any(|(i, &byte)| {
            (byte == first || byte == first_upper)
                && after_first[i..i + rest.len()]
                    .iter()
                    .zip(rest)
                    .all(|(&t, &w)| t.to_ascii_lowercase() == w)
        })
    } else {
        text.to_lowercase().contains(word)
    }
}

/// Byte ranges of `text` where `word` (lowercase) is, compared letter by letter in
/// lowercase, so the ranges are of `text` even where lowercasing changes the length.
pub(crate) fn find_ignoring_case(text: &str, word: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut from = 0;
    'starts: for (start, _) in text.char_indices() {
        if start < from {
            continue;
        }
        let mut want = needle.iter().peekable();
        for (i, c) in text[start..].char_indices() {
            for lower in c.to_lowercase() {
                match want.next() {
                    Some(&w) if w == lower => {}
                    _ => continue 'starts,
                }
            }
            if want.peek().is_none() {
                let end = start + i + c.len_utf8();
                out.push(start..end);
                from = end;
                continue 'starts;
            }
        }
    }
    out
}

#[cfg(test)]
mod query_tests {
    use super::{TextQuery, contains_ignoring_case, find_ignoring_case};

    #[test]
    fn matches_ignore_letter_case_in_and_beyond_ascii() {
        assert!(contains_ignoring_case("Report.PDF", "pdf"));
        assert!(!contains_ignoring_case("Report", "reports"));
        assert!(contains_ignoring_case("ÄRGER", "ärg"));
        assert!(!contains_ignoring_case("Arger", "ä"));
    }

    #[test]
    fn finds_lowercase_words_as_comparing_every_window_does() {
        // Reference: compares at every start, without the first-letter search.
        fn every_window(text: &str, word: &str) -> bool {
            if text.is_ascii() {
                let (text, word) = (text.as_bytes(), word.as_bytes());
                word.is_empty()
                    || text
                        .windows(word.len())
                        .any(|window| window.eq_ignore_ascii_case(word))
            } else {
                text.to_lowercase().contains(word)
            }
        }
        let texts = [
            "",
            "a",
            "A",
            "File 004217.txt",
            "REPORT.pdf",
            "2026-03-17",
            "aaaAAb",
            "x-ray X-RAY",
            "[brackets] {and} @symbols ^_`",
            "ÄRGER über Öl",
        ];
        let words = [
            "",
            "a",
            "b",
            "aab",
            "file",
            "txt",
            ".txt",
            "0042",
            "2026-03",
            "report.pdf",
            "x-r",
            "[",
            "@",
            "^_`",
            "z",
            "ärg",
            "über",
            "file 004217.txt!",
            "aaaaab",
        ];
        for text in texts {
            for word in words {
                assert_eq!(
                    contains_ignoring_case(text, word),
                    every_window(text, word),
                    "{text:?} {word:?}"
                );
            }
        }
        // Ranges are of the text as given, also where a letter is several bytes.
        assert_eq!(find_ignoring_case("Ärger ärger", "är"), [0..3, 7..10]);
        assert_eq!(find_ignoring_case("aaaa", "aa"), [0..2, 2..4]);
        assert_eq!(
            find_ignoring_case("日本語の表", "本語").first(),
            Some(&(3..9))
        );
        assert!(find_ignoring_case("abc", "x").is_empty());
    }

    #[test]
    fn a_query_needs_every_word_and_marks_overlapping_words_once() {
        let mut query = TextQuery::default();
        assert!(query.set("Rep  PORT x"));
        assert!(!query.set("Rep  PORT x"), "the same text is no change");
        assert_eq!(query.words(), ["rep", "port", "x"]);
        assert!(query.matches("Report export"));
        assert!(!query.matches("Report"));
        assert_eq!(query.ranges("Report export"), [0..6, 8..13]);
        assert!(query.set(" "));
        assert!(query.is_empty() && query.matches("anything"));
    }
}
