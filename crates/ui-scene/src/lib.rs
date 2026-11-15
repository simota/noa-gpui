//! The [`DisplayList`] is the boundary between the UI tree and the GPU renderer.
//! This crate must not depend on any GPU or windowing API.

pub mod batch;
pub mod clipping;
pub mod display_list;
pub mod geometry;
pub mod vector;

pub use batch::{Batch, BatchKind, Batcher};
pub use clipping::{Clip, ClipStack};
pub use display_list::{
    Border, CornerRadii, Direction, DisplayList, DrawCommand, ImageId, Shape, TextLayoutId,
};
pub use geometry::{Color, Point, Rect, Size};
pub use vector::{FillRule, IconBuilder, IconError, IconLayer, IconPaint, PathCommand, VectorIcon};
