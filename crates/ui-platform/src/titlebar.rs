//! How a window's title bar looks, and (macOS) where its close, minimize and zoom
//! buttons sit. AppKit lays the buttons out again when the window resizes, enters or
//! leaves full screen or becomes key, so they are moved back after each of those.

use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::window::{Window, WindowAttributes};

/// The title bar of a window (see [`crate::AppOptions::titlebar`] and
/// [`crate::WindowRequest::titlebar`]). The default is the OS's own. Only macOS reads
/// these: elsewhere drop the decorations and draw a title bar instead (see
/// [`crate::AppOptions::decorations`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Titlebar {
    /// The window's content reaches under a see-through title bar: the screen draws
    /// what is behind the title and the buttons.
    pub transparent: bool,
    /// No title text (the window keeps its title for the Window menu and Mission
    /// Control).
    pub hide_title: bool,
    /// Where the close button's top-left corner sits, in logical pixels from the
    /// window's top-left; the minimize and zoom buttons follow at the usual spacing.
    /// `None` leaves them where AppKit puts them.
    pub traffic_lights: Option<(f64, f64)>,
}

/// `attributes` with the title bar and the smallest size a window may be resized to.
pub(crate) fn apply(
    attributes: WindowAttributes,
    titlebar: &Titlebar,
    min_size: Option<(f64, f64)>,
) -> WindowAttributes {
    let attributes = match min_size {
        Some((width, height)) => attributes.with_min_inner_size(LogicalSize::new(width, height)),
        None => attributes,
    };
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowAttributesExtMacOS;
        attributes
            .with_titlebar_transparent(titlebar.transparent)
            .with_fullsize_content_view(titlebar.transparent)
            .with_title_hidden(titlebar.hide_title)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = titlebar;
        attributes
    }
}

/// Put the buttons back where `titlebar` wants them after an event that makes AppKit
/// lay them out again; `None` places them now (a new window).
pub(crate) fn follow(window: &Window, titlebar: &Titlebar, event: Option<&WindowEvent>) {
    let relayout = event.is_none_or(|event| {
        matches!(
            event,
            WindowEvent::Resized(_)
                | WindowEvent::Focused(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::Occluded(false)
        )
    });
    let Some(at) = titlebar.traffic_lights.filter(|_| relayout) else {
        return;
    };
    #[cfg(all(feature = "native", target_os = "macos"))]
    macos::place_traffic_lights(window, at);
    #[cfg(not(all(feature = "native", target_os = "macos")))]
    let _ = (window, at);
}

/// Origins of the close, minimize and zoom buttons in the title bar view (y up, as
/// AppKit's views are): the close button's top-left at `at` (y down from the window's
/// top), the others `spacing` apart as AppKit spaced them.
fn traffic_light_origins(
    titlebar_height: f64,
    button_height: f64,
    spacing: f64,
    at: (f64, f64),
) -> [(f64, f64); 3] {
    let y = titlebar_height - at.1 - button_height;
    [0.0, 1.0, 2.0].map(|i| (at.0 + i * spacing, y))
}

#[cfg(all(feature = "native", target_os = "macos"))]
mod macos {
    use objc2::encode::{Encode, Encoding};
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    // SAFETY: the layouts and names are Core Graphics' (CGFloat is f64 on 64-bit).
    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for CGRect {
        const ENCODING: Encoding =
            Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }

    /// NSWindowCloseButton, NSWindowMiniaturizeButton, NSWindowZoomButton.
    const BUTTONS: [usize; 3] = [0, 1, 2];

    pub(super) fn place_traffic_lights(window: &Window, at: (f64, f64)) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        let view = handle.ns_view.as_ptr().cast::<AnyObject>();
        // SAFETY: winit's view is alive while its window is, and this runs on the main
        // thread (window events do). The messages are NSView's `window` and NSWindow's
        // `frame`, `contentLayoutRect` and `standardWindowButton:`, and NSView's `frame`
        // and `setFrameOrigin:`, with the argument and return types they declare.
        unsafe {
            let ns_window: *mut AnyObject = msg_send![view, window];
            if ns_window.is_null() {
                return;
            }
            let frame: CGRect = msg_send![ns_window, frame];
            let content: CGRect = msg_send![ns_window, contentLayoutRect];
            let buttons: [*mut AnyObject; 3] =
                BUTTONS.map(|kind| msg_send![ns_window, standardWindowButton: kind]);
            if buttons.iter().any(|b| b.is_null()) {
                return;
            }
            let close: CGRect = msg_send![buttons[0], frame];
            let minimize: CGRect = msg_send![buttons[1], frame];
            let origins = super::traffic_light_origins(
                frame.size.height - content.size.height,
                close.size.height,
                minimize.origin.x - close.origin.x,
                at,
            );
            for (button, (x, y)) in buttons.into_iter().zip(origins) {
                let _: () = msg_send![button, setFrameOrigin: CGPoint { x, y }];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_hang_from_the_given_point_at_their_spacing() {
        // A 28 pt title bar, 14 pt buttons 20 pt apart, the close button at (14, 7).
        let origins = traffic_light_origins(28.0, 14.0, 20.0, (14.0, 7.0));
        assert_eq!(origins, [(14.0, 7.0), (34.0, 7.0), (54.0, 7.0)]);
        // Lower in a taller title bar: y counts up from its bottom.
        let origins = traffic_light_origins(52.0, 14.0, 20.0, (14.0, 14.0));
        assert_eq!(origins[0], (14.0, 24.0));
    }

    #[test]
    fn a_minimum_size_limits_resizing() {
        let attributes = apply(
            Window::default_attributes(),
            &Titlebar::default(),
            Some((640.0, 400.0)),
        );
        assert_eq!(
            attributes.min_inner_size,
            Some(LogicalSize::new(640.0, 400.0).into())
        );
        let free = apply(Window::default_attributes(), &Titlebar::default(), None);
        assert_eq!(free.min_inner_size, None);
    }
}
