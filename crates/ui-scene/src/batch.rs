//! A batch is a run of commands that the renderer can draw with one pipeline, one
//! set of bindings and one scissor rectangle. Commands are batched in display-list
//! order. A command may join an *earlier* batch with the same key only when it does
//! not overlap anything drawn by the batches after it, so the composited result is
//! identical to painting the commands strictly in order.
//!
//! Overlap is tested against a uniform grid of already-batched items, so batching a
//! frame costs roughly linear time in the number of commands.

use crate::clipping::{Clip, ClipStack};
use crate::display_list::{DisplayList, DrawCommand, ImageId};
use crate::geometry::Rect;

/// How many trailing batches a command may skip over to join an earlier batch.
const MAX_LOOKBACK: usize = 16;

/// Edge length of an overlap grid cell, in logical pixels.
const GRID_CELL: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchKind {
    Quad,
    Image(ImageId),
    Icon,
    Text,
    /// One [`DrawCommand::Backdrop`]. Never merged, and nothing after it merges into a
    /// batch before it: the blur must see exactly what was drawn earlier.
    Backdrop,
}

#[derive(Debug, Clone)]
pub struct Batch {
    pub kind: BatchKind,
    /// Scissor rectangle in logical pixels; `None` means the whole target.
    pub clip: Option<Rect>,
    /// Indices into [`DisplayList::commands`], in drawing order.
    commands: Vec<u32>,
}

impl Batch {
    pub fn commands(&self) -> &[u32] {
        &self.commands
    }
}

/// Visible areas of batched items, bucketed by grid cell, tagged with their batch. The
/// items of a cell are ordered by batch.
#[derive(Debug, Default)]
struct OverlapGrid {
    origin: (f32, f32),
    cols: usize,
    rows: usize,
    cells: Vec<Vec<(u32, Rect)>>,
}

impl OverlapGrid {
    fn reset(&mut self, area: Rect) {
        self.origin = (area.x, area.y);
        self.cols = ((area.width / GRID_CELL).ceil() as usize).max(1);
        self.rows = ((area.height / GRID_CELL).ceil() as usize).max(1);
        let len = self.cols * self.rows;
        self.cells.iter_mut().for_each(Vec::clear);
        self.cells.resize_with(len, Vec::new);
    }

    /// Cell index ranges covered by `rect` (which lies inside the grid's area).
    fn span(&self, rect: &Rect) -> Span {
        // `as` rounds toward zero and turns negative values and NaN into 0: the floor,
        // clamped at 0, without a call to `floorf`.
        let cell =
            |v: f32, origin: f32, max: usize| (((v - origin) / GRID_CELL) as usize).min(max - 1);
        let x0 = cell(rect.x, self.origin.0, self.cols);
        let x1 = cell(rect.right(), self.origin.0, self.cols);
        let y0 = cell(rect.y, self.origin.1, self.rows);
        let y1 = cell(rect.bottom(), self.origin.1, self.rows);
        (x0..x1 + 1, y0..y1 + 1)
    }

    fn insert(&mut self, (xs, ys): &Span, rect: Rect, batch: u32) {
        for y in ys.clone() {
            for x in xs.clone() {
                let cell = &mut self.cells[y * self.cols + x];
                // Kept in batch order (see `overlaps_after`). Items join recent
                // batches, so this is nearly always the end.
                cell.push((batch, rect));
                // Step back past the items of later batches: as many as a query
                // over this cell looks at.
                let mut at = cell.len() - 1;
                while at > 0 && cell[at - 1].0 > batch {
                    cell.swap(at - 1, at);
                    at -= 1;
                }
            }
        }
    }

    /// Each cell's items are in batch order, so only its tail of items after `batch`
    /// is looked at: a cell under many layers (windows stacked on each other) holds
    /// mostly items of earlier batches.
    fn overlaps_after(&self, (xs, ys): &Span, rect: &Rect, batch: u32) -> bool {
        ys.clone().any(|y| {
            xs.clone().any(|x| {
                (self.cells[y * self.cols + x].iter().rev())
                    .take_while(|(b, _)| *b > batch)
                    .any(|(_, r)| r.intersects(rect))
            })
        })
    }
}

/// (columns, rows) of grid cells.
type Span = (std::ops::Range<usize>, std::ops::Range<usize>);

/// Reuses allocations across frames.
#[derive(Debug, Default)]
pub struct Batcher {
    batches: Vec<Batch>,
    spare: Vec<Batch>,
    clips: ClipStack,
    grid: OverlapGrid,
    culled: usize,
    /// Batches before this index are closed to merging (they precede a backdrop).
    barrier: usize,
}

impl Batcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// `viewport` is the visible area in logical pixels. Commands entirely outside it or
    /// the active clip are culled.
    pub fn build(&mut self, list: &DisplayList, viewport: Rect) -> &[Batch] {
        self.recycle();
        self.clips.clear();
        self.grid.reset(viewport);
        self.culled = 0;
        self.barrier = 0;

        for (index, command) in list.commands().iter().enumerate() {
            let kind = match command {
                DrawCommand::PushClip { rect } => {
                    self.clips.push(*rect);
                    continue;
                }
                DrawCommand::PopClip => {
                    self.clips.pop();
                    continue;
                }
                DrawCommand::Quad { .. } | DrawCommand::Shape { .. } => BatchKind::Quad,
                DrawCommand::Image { image, .. } => BatchKind::Image(*image),
                DrawCommand::Icon { .. } => BatchKind::Icon,
                DrawCommand::Text { .. } => BatchKind::Text,
                DrawCommand::Backdrop { .. } => BatchKind::Backdrop,
            };
            let bounds = command.bounds().expect("drawing commands have bounds");

            let (clip, visible_area) = match self.clips.current() {
                Clip::None => (None, viewport),
                Clip::Rect(rect) => (Some(rect), rect),
                Clip::Empty => {
                    self.culled += 1;
                    continue;
                }
            };
            let Some(visible) = bounds
                .intersect(&visible_area)
                .and_then(|r| r.intersect(&viewport))
            else {
                self.culled += 1;
                continue;
            };

            self.insert(kind, clip, index as u32, visible);
        }

        debug_assert_eq!(self.clips.depth(), 0, "unbalanced PushClip/PopClip");
        &self.batches
    }

    pub fn batches(&self) -> &[Batch] {
        &self.batches
    }

    /// Commands culled in the last build.
    pub fn culled(&self) -> usize {
        self.culled
    }

    fn insert(&mut self, kind: BatchKind, clip: Option<Rect>, command: u32, visible: Rect) {
        let start = self
            .batches
            .len()
            .saturating_sub(MAX_LOOKBACK)
            .max(self.barrier);
        let span = self.grid.span(&visible);
        // Only the most recent batch with the same key is a candidate: an older one has
        // at least as many batches after it to overlap with.
        let target = (start..self.batches.len())
            .rev()
            .find(|&i| self.batches[i].kind == kind && self.batches[i].clip == clip)
            .filter(|&i| !self.grid.overlaps_after(&span, &visible, i as u32))
            .filter(|_| kind != BatchKind::Backdrop);

        let index = match target {
            Some(i) => i,
            None => {
                let mut batch = self.spare.pop().unwrap_or_else(|| Batch {
                    kind,
                    clip,
                    commands: Vec::new(),
                });
                batch.kind = kind;
                batch.clip = clip;
                self.batches.push(batch);
                self.batches.len() - 1
            }
        };
        self.batches[index].commands.push(command);
        self.grid.insert(&span, visible, index as u32);
        if kind == BatchKind::Backdrop {
            self.barrier = self.batches.len();
        }
    }

    fn recycle(&mut self) {
        for mut batch in self.batches.drain(..) {
            batch.commands.clear();
            self.spare.push(batch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display_list::{Border, TextLayoutId};
    use crate::geometry::Color;

    const VIEWPORT: Rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);

    fn quad(list: &mut DisplayList, x: f32, y: f32) {
        list.rounded_rect(
            Rect::new(x, y, 10.0, 10.0),
            2.0,
            Color::WHITE,
            Border::default(),
        );
    }

    fn summary(batches: &[Batch]) -> Vec<(BatchKind, Vec<u32>)> {
        batches
            .iter()
            .map(|b| (b.kind, b.commands().to_vec()))
            .collect()
    }

    /// The batching rules without the grid: every earlier item is checked for overlap.
    fn reference(list: &DisplayList, viewport: Rect) -> Vec<(BatchKind, Option<Rect>, Vec<u32>)> {
        let mut batches: Vec<(BatchKind, Option<Rect>, Vec<u32>)> = Vec::new();
        let mut items: Vec<(usize, Rect)> = Vec::new();
        let mut clips = ClipStack::default();
        let mut barrier = 0;
        for (index, command) in list.commands().iter().enumerate() {
            let kind = match command {
                DrawCommand::PushClip { rect } => {
                    clips.push(*rect);
                    continue;
                }
                DrawCommand::PopClip => {
                    clips.pop();
                    continue;
                }
                DrawCommand::Quad { .. } | DrawCommand::Shape { .. } => BatchKind::Quad,
                DrawCommand::Image { image, .. } => BatchKind::Image(*image),
                DrawCommand::Icon { .. } => BatchKind::Icon,
                DrawCommand::Text { .. } => BatchKind::Text,
                DrawCommand::Backdrop { .. } => BatchKind::Backdrop,
            };
            let (clip, area) = match clips.current() {
                Clip::None => (None, viewport),
                Clip::Rect(rect) => (Some(rect), rect),
                Clip::Empty => continue,
            };
            let Some(visible) =
                (command.bounds().unwrap().intersect(&area)).and_then(|r| r.intersect(&viewport))
            else {
                continue;
            };
            let start = batches.len().saturating_sub(MAX_LOOKBACK).max(barrier);
            let target = (start..batches.len())
                .rev()
                .find(|&i| batches[i].0 == kind && batches[i].1 == clip)
                .filter(|&i| !items.iter().any(|(b, r)| *b > i && r.intersects(&visible)))
                .filter(|_| kind != BatchKind::Backdrop);
            let at = target.unwrap_or_else(|| {
                batches.push((kind, clip, Vec::new()));
                batches.len() - 1
            });
            batches[at].2.push(index as u32);
            items.push((at, visible));
            if kind == BatchKind::Backdrop {
                barrier = batches.len();
            }
        }
        batches
    }

    #[test]
    fn grid_batching_matches_checking_every_earlier_item() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |n: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % u64::from(n)) as f32
        };
        let viewport = Rect::new(0.0, 0.0, 640.0, 480.0);
        let mut batcher = Batcher::new();
        for round in 0..60 {
            let mut list = DisplayList::new();
            let mut depth = 0;
            for _ in 0..(50 + round * 40) {
                let rect = match next(10) as u32 {
                    0 => Rect::new(next(700) - 30.0, next(520) - 20.0, next(400), next(300)),
                    _ => Rect::new(next(660) - 10.0, next(500) - 10.0, next(60), next(40)),
                };
                match next(24) as u32 {
                    0..=7 => list.rounded_rect(rect, 2.0, Color::WHITE, Border::default()),
                    8..=13 => list.text(rect, TextLayoutId(1), Color::BLACK),
                    14..=15 => list.image(rect, ImageId(next(3) as u32)),
                    16 if round % 3 == 0 => list.push(DrawCommand::Backdrop {
                        rect,
                        radius: 4.0,
                        blur: 6.0,
                        tint: Color::WHITE,
                    }),
                    17..=18 if depth < 4 => {
                        list.push_clip(rect);
                        depth += 1;
                    }
                    19..=20 if depth > 0 => {
                        list.pop_clip();
                        depth -= 1;
                    }
                    _ => list.fill_rect(rect, Color::BLACK),
                }
            }
            for _ in 0..depth {
                list.pop_clip();
            }
            let got: Vec<_> = (batcher.build(&list, viewport).iter())
                .map(|b| (b.kind, b.clip, b.commands().to_vec()))
                .collect();
            assert_eq!(got, reference(&list, viewport), "round {round}");
        }
    }

    #[test]
    fn grid_cells_are_the_floor_clamped_to_the_grid() {
        let mut grid = OverlapGrid::default();
        grid.reset(Rect::new(-16.0, 0.0, 128.0, 64.0)); // 4 x 2 cells
        let span = |x: f32, w: f32| grid.span(&Rect::new(x, 10.0, w, 10.0)).0;
        assert_eq!(span(-16.0, 31.9), 0..1);
        assert_eq!(span(15.9, 0.2), 0..2, "-16 + 32 starts the second cell");
        assert_eq!(span(-100.0, 10.0), 0..1, "left of the grid: the first cell");
        assert_eq!(
            span(-17.0, 0.5),
            0..1,
            "a fraction of a cell left of it too"
        );
        assert_eq!(span(50.0, 1000.0), 2..4, "right of the grid: the last cell");
        assert_eq!(span(f32::NAN, 1.0), 0..1);
        assert_eq!(grid.span(&Rect::new(0.0, 31.0, 1.0, 2.0)).1, 0..2);
    }

    #[test]
    fn consecutive_quads_share_a_batch() {
        let mut list = DisplayList::new();
        quad(&mut list, 0.0, 0.0);
        quad(&mut list, 20.0, 0.0);
        let mut batcher = Batcher::new();
        let batches = batcher.build(&list, VIEWPORT);
        assert_eq!(summary(batches), vec![(BatchKind::Quad, vec![0, 1])]);
    }

    #[test]
    fn non_overlapping_widgets_merge_across_kinds() {
        let mut list = DisplayList::new();
        quad(&mut list, 0.0, 0.0);
        list.text(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            TextLayoutId(0),
            Color::BLACK,
        );
        quad(&mut list, 20.0, 0.0);
        list.text(
            Rect::new(20.0, 0.0, 10.0, 10.0),
            TextLayoutId(1),
            Color::BLACK,
        );

        let mut batcher = Batcher::new();
        let batches = batcher.build(&list, VIEWPORT);
        assert_eq!(
            summary(batches),
            vec![(BatchKind::Quad, vec![0, 2]), (BatchKind::Text, vec![1, 3])]
        );
    }

    #[test]
    fn overlapping_command_keeps_paint_order() {
        // quad A, text over A, quad B over the text: B must stay above the text.
        let mut list = DisplayList::new();
        quad(&mut list, 0.0, 0.0);
        list.text(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            TextLayoutId(0),
            Color::BLACK,
        );
        quad(&mut list, 5.0, 5.0);

        let mut batcher = Batcher::new();
        let batches = batcher.build(&list, VIEWPORT);
        assert_eq!(
            summary(batches),
            vec![
                (BatchKind::Quad, vec![0]),
                (BatchKind::Text, vec![1]),
                (BatchKind::Quad, vec![2]),
            ]
        );
    }

    #[test]
    fn different_images_do_not_share_a_batch() {
        let mut list = DisplayList::new();
        list.image(Rect::new(0.0, 0.0, 10.0, 10.0), ImageId(0));
        list.image(Rect::new(20.0, 0.0, 10.0, 10.0), ImageId(1));
        list.image(Rect::new(40.0, 0.0, 10.0, 10.0), ImageId(0));

        let mut batcher = Batcher::new();
        let batches = batcher.build(&list, VIEWPORT);
        assert_eq!(
            summary(batches),
            vec![
                (BatchKind::Image(ImageId(0)), vec![0, 2]),
                (BatchKind::Image(ImageId(1)), vec![1]),
            ]
        );
    }

    #[test]
    fn clip_splits_batches_and_culls() {
        let mut list = DisplayList::new();
        quad(&mut list, 0.0, 0.0);
        list.push_clip(Rect::new(0.0, 100.0, 50.0, 50.0));
        quad(&mut list, 0.0, 110.0); // inside clip
        quad(&mut list, 200.0, 110.0); // outside clip: culled
        list.pop_clip();
        quad(&mut list, 20.0, 0.0);
        quad(&mut list, 2000.0, 0.0); // outside viewport: culled

        let mut batcher = Batcher::new();
        let batches = batcher.build(&list, VIEWPORT);
        assert_eq!(
            summary(batches),
            vec![(BatchKind::Quad, vec![0, 5]), (BatchKind::Quad, vec![2])]
        );
        assert_eq!(batches[1].clip, Some(Rect::new(0.0, 100.0, 50.0, 50.0)));
        assert_eq!(batcher.culled(), 2);
    }

    #[test]
    fn grid_of_widgets_collapses_to_three_batches() {
        let mut list = DisplayList::new();
        for i in 0..1200 {
            let x = (i % 40) as f32 * 25.0;
            let y = (i / 40) as f32 * 25.0;
            quad(&mut list, x, y);
            list.image(Rect::new(x + 1.0, y + 1.0, 4.0, 4.0), ImageId(0));
            list.text(
                Rect::new(x + 5.0, y, 5.0, 10.0),
                TextLayoutId(i),
                Color::BLACK,
            );
        }
        let mut batcher = Batcher::new();
        let batches = batcher.build(&list, VIEWPORT);
        let kinds: Vec<_> = batches.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BatchKind::Quad,
                BatchKind::Image(ImageId(0)),
                BatchKind::Text
            ]
        );
        assert!(batches.iter().all(|b| b.commands().len() == 1200));
    }

    #[test]
    fn large_item_spanning_many_cells_blocks_merge() {
        // A full-width text banner sits between two quads; the second quad overlaps the
        // banner far from the banner's origin cell.
        let mut list = DisplayList::new();
        quad(&mut list, 0.0, 0.0);
        list.text(
            Rect::new(0.0, 100.0, 900.0, 20.0),
            TextLayoutId(0),
            Color::BLACK,
        );
        quad(&mut list, 850.0, 105.0);

        let mut batcher = Batcher::new();
        let batches = batcher.build(&list, VIEWPORT);
        assert_eq!(
            summary(batches),
            vec![
                (BatchKind::Quad, vec![0]),
                (BatchKind::Text, vec![1]),
                (BatchKind::Quad, vec![2]),
            ]
        );
    }

    #[test]
    fn rebuild_reuses_batches() {
        let mut list = DisplayList::new();
        quad(&mut list, 0.0, 0.0);
        let mut batcher = Batcher::new();
        batcher.build(&list, VIEWPORT);
        list.clear();
        list.text(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            TextLayoutId(0),
            Color::BLACK,
        );
        let batches = batcher.build(&list, VIEWPORT);
        assert_eq!(summary(batches), vec![(BatchKind::Text, vec![0])]);
    }
}
