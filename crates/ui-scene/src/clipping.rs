use crate::geometry::Rect;

/// The effective clip is the intersection of all pushed rectangles.
#[derive(Debug, Clone, Default)]
pub struct ClipStack {
    /// Effective clip after each push. `None` means the pushed region is empty, so
    /// everything inside it is invisible.
    stack: Vec<Option<Rect>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Clip {
    /// The whole target is drawable.
    None,
    Rect(Rect),
    /// Nothing is visible.
    Empty,
}

impl ClipStack {
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn current(&self) -> Clip {
        match self.stack.last() {
            None => Clip::None,
            Some(Some(rect)) => Clip::Rect(*rect),
            Some(None) => Clip::Empty,
        }
    }

    pub fn push(&mut self, rect: Rect) {
        let next = match self.current() {
            Clip::None => (!rect.is_empty()).then_some(rect),
            Clip::Rect(current) => current.intersect(&rect),
            Clip::Empty => None,
        };
        self.stack.push(next);
    }

    pub fn pop(&mut self) {
        let popped = self.stack.pop();
        debug_assert!(popped.is_some(), "PopClip without matching PushClip");
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_clips_intersect() {
        let mut clips = ClipStack::default();
        assert_eq!(clips.current(), Clip::None);
        clips.push(Rect::new(0.0, 0.0, 100.0, 100.0));
        clips.push(Rect::new(50.0, 50.0, 100.0, 100.0));
        assert_eq!(
            clips.current(),
            Clip::Rect(Rect::new(50.0, 50.0, 50.0, 50.0))
        );
        clips.pop();
        assert_eq!(
            clips.current(),
            Clip::Rect(Rect::new(0.0, 0.0, 100.0, 100.0))
        );
        clips.pop();
        assert_eq!(clips.current(), Clip::None);
    }

    #[test]
    fn disjoint_clips_are_empty_until_popped() {
        let mut clips = ClipStack::default();
        clips.push(Rect::new(0.0, 0.0, 10.0, 10.0));
        clips.push(Rect::new(20.0, 20.0, 10.0, 10.0));
        assert_eq!(clips.current(), Clip::Empty);
        clips.push(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(clips.current(), Clip::Empty);
        clips.pop();
        clips.pop();
        assert_eq!(clips.current(), Clip::Rect(Rect::new(0.0, 0.0, 10.0, 10.0)));
    }
}
