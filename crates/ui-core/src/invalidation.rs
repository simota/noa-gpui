//! What must be recomputed after a change.
//!
//! | Flag        | Work                                              | Example                    |
//! | ----------- | ------------------------------------------------- | -------------------------- |
//! | `LAYOUT`    | re-measure / re-layout, then repaint what resized | text content changed       |
//! | `PAINT`     | regenerate one widget's draw commands             | background color changed   |
//! | `COMPOSITE` | reassemble the display list from cached commands  | scroll offset changed      |
//!
//! Each widget keeps its own draw commands in local coordinates, so moving a widget
//! (layout position or scroll offset) only needs `COMPOSITE`, never a repaint.

use std::ops::{BitOr, BitOrAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Dirty(u8);

impl Dirty {
    pub const NONE: Dirty = Dirty(0);
    pub const LAYOUT: Dirty = Dirty(1);
    pub const PAINT: Dirty = Dirty(2);
    pub const COMPOSITE: Dirty = Dirty(4);

    pub const fn contains(self, other: Dirty) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Dirty) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn remove(&mut self, other: Dirty) {
        self.0 &= !other.0;
    }
}

impl BitOr for Dirty {
    type Output = Dirty;
    fn bitor(self, rhs: Dirty) -> Dirty {
        Dirty(self.0 | rhs.0)
    }
}

impl BitOrAssign for Dirty {
    fn bitor_assign(&mut self, rhs: Dirty) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_combine_and_clear() {
        let mut d = Dirty::LAYOUT | Dirty::COMPOSITE;
        assert!(d.contains(Dirty::LAYOUT));
        assert!(!d.contains(Dirty::PAINT));
        assert!(d.intersects(Dirty::PAINT | Dirty::COMPOSITE));
        d.remove(Dirty::LAYOUT);
        assert_eq!(d, Dirty::COMPOSITE);
        d.remove(Dirty::COMPOSITE);
        assert!(d.is_empty());
    }
}
