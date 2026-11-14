use ui_layout::Style;
use ui_layout::style::{Size as LayoutSize, length};
use ui_scene::{Border, Color, ImageId, Rect, Size};

use crate::theme::Role;
use crate::widget::{PaintCx, UpdateCx, Widget};

/// How an [`Image`] fits its picture into its box (`NSImageScaling`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageScaling {
    /// Stretch to fill the box, ignoring the aspect ratio.
    #[default]
    Stretch,
    /// Scale up or down to fit inside, keeping the aspect ratio (letterboxed).
    Fit,
    /// Like [`ImageScaling::Fit`], but never larger than the picture's own size.
    FitDown,
    /// Scale to cover the box, keeping the aspect ratio; the overflow is cut off.
    Fill,
    /// The picture's own size, centered; cut off if larger than the box.
    None,
}

/// A frame drawn around an [`Image`] (`NSImageFrameStyle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageFrame {
    #[default]
    None,
    Photo,
    Bezel,
    Groove,
}

impl ImageFrame {
    /// Space the frame takes on each side.
    pub fn inset(self) -> f32 {
        match self {
            ImageFrame::None => 0.0,
            ImageFrame::Photo => 6.0,
            ImageFrame::Bezel => 4.0,
            ImageFrame::Groove => 3.0,
        }
    }
}

/// Where a picture of `natural` size goes in `area` with `scaling` (it may extend past
/// `area` for [`ImageScaling::Fill`] and [`ImageScaling::None`]; draw it clipped).
pub fn image_rect(scaling: ImageScaling, natural: Size, area: Rect) -> Rect {
    let centered = |w: f32, h: f32| {
        Rect::new(
            area.x + (area.width - w) / 2.0,
            area.y + (area.height - h) / 2.0,
            w,
            h,
        )
    };
    if natural.width <= 0.0 || natural.height <= 0.0 {
        return area;
    }
    let sx = area.width / natural.width;
    let sy = area.height / natural.height;
    match scaling {
        ImageScaling::Stretch => area,
        ImageScaling::Fit => {
            let s = sx.min(sy);
            centered(natural.width * s, natural.height * s)
        }
        ImageScaling::FitDown => {
            let s = sx.min(sy).min(1.0);
            centered(natural.width * s, natural.height * s)
        }
        ImageScaling::Fill => {
            let s = sx.max(sy);
            centered(natural.width * s, natural.height * s)
        }
        ImageScaling::None => centered(natural.width, natural.height),
    }
}

/// An image in a fixed-size box (`NSImageView`). Stretched to the box by default; the
/// aspect-preserving scalings need [`Image::natural_size`].
#[derive(Debug, Clone)]
pub struct Image {
    image: ImageId,
    size: Size,
    natural: Size,
    scaling: ImageScaling,
    frame: ImageFrame,
}

impl Image {
    pub fn new(image: ImageId, size: Size) -> Self {
        Self {
            image,
            size,
            natural: size,
            scaling: ImageScaling::Stretch,
            frame: ImageFrame::None,
        }
    }

    /// Logical pixels.
    pub fn natural_size(mut self, natural: Size) -> Self {
        self.natural = natural;
        self
    }

    pub fn scaling(mut self, scaling: ImageScaling) -> Self {
        self.scaling = scaling;
        self
    }

    pub fn frame(mut self, frame: ImageFrame) -> Self {
        self.frame = frame;
        self
    }

    /// Widget coordinates.
    pub fn picture_rect(&self) -> Rect {
        image_rect(self.scaling, self.natural, self.content_area())
    }

    /// The area inside the frame. A photo's mat stops 2 px short of its shadow.
    fn content_area(&self) -> Rect {
        let i = self.frame.inset();
        let shadow = if self.frame == ImageFrame::Photo {
            2.0
        } else {
            0.0
        };
        Rect::new(
            i,
            i,
            (self.size.width - 2.0 * i - shadow).max(0.0),
            (self.size.height - 2.0 * i - shadow).max(0.0),
        )
    }

    pub fn set_image(&mut self, cx: &mut UpdateCx, image: ImageId) {
        if image != self.image {
            self.image = image;
            cx.request_paint();
        }
    }

    fn paint_frame(&self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let outline = cx.color(Role::Outline);
        match self.frame {
            ImageFrame::None => {}
            ImageFrame::Photo => {
                let shadow = cx.color(Color::BLACK.with_alpha(0.25));
                cx.fill_rect(
                    Rect::new(2.0, 3.0, bounds.width - 2.0, bounds.height - 2.0),
                    shadow,
                );
                let mat = cx.color(Color::WHITE);
                cx.rounded_rect(
                    Rect::new(0.0, 0.0, bounds.width - 2.0, bounds.height - 2.0),
                    0.0,
                    mat,
                    Border {
                        width: 1.0,
                        color: outline,
                    },
                );
            }
            ImageFrame::Bezel => {
                let well = cx.color(Role::SurfaceAlt);
                cx.rounded_rect(
                    bounds,
                    6.0,
                    well,
                    Border {
                        width: 1.0,
                        color: outline,
                    },
                );
            }
            ImageFrame::Groove => {
                let light = cx.color(Color::WHITE.with_alpha(0.35));
                let dark = cx.color(Color::BLACK.with_alpha(0.35));
                let line = |width: f32, color: Color| Border { width, color };
                cx.rounded_rect(bounds, 0.0, Color::TRANSPARENT, line(1.0, dark));
                cx.rounded_rect(
                    Rect::new(1.0, 1.0, bounds.width - 2.0, bounds.height - 2.0),
                    0.0,
                    Color::TRANSPARENT,
                    line(1.0, light),
                );
            }
        }
    }
}

impl Widget for Image {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: length(self.size.width),
                height: length(self.size.height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        self.paint_frame(cx);
        let area = self.content_area();
        let picture = self.picture_rect();
        let clip = matches!(self.scaling, ImageScaling::Fill | ImageScaling::None);
        if clip {
            cx.push_clip(area);
        }
        cx.image(picture, self.image);
        if clip {
            cx.pop_clip();
        }
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Image)
    }
}
