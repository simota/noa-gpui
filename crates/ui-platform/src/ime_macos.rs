//! Answers to the questions an input method asks about the text (NSTextInputClient),
//! which winit leaves unanswered: what is selected, the text of a range, the
//! character under a point and the rectangle of a range. Without them reconversion
//! has nothing to convert and the candidate window stays where the caret was.
//!
//! winit's view class gets these methods replaced once (the originals answer while no
//! focused widget gives an [`ImeContext`]). AppKit asks while winit handles a key, when
//! the application cannot be reached, so the answers come from a copy of the focused
//! widget's context taken after every frame ([`sync`]). Ranges are UTF-16 code units
//! of [`ImeContext::text`]; the input method only relates them to one another.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use noa_gpui_core::ImeContext;
use noa_gpui_scene::{Point, Rect};
use objc2::encode::{Encode, Encoding};
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{msg_send, sel};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// NSNotFound.
const NOT_FOUND: usize = isize::MAX as usize;
/// NSUTF8StringEncoding.
const UTF8: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct NSRange {
    location: usize,
    length: usize,
}

impl NSRange {
    const NONE: Self = Self {
        location: NOT_FOUND,
        length: 0,
    };

    fn of(range: Range<usize>) -> Self {
        Self {
            location: range.start,
            length: range.end - range.start,
        }
    }

    fn range(self) -> Option<Range<usize>> {
        (self.location != NOT_FOUND).then(|| self.location..self.location + self.length)
    }
}

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

// SAFETY: the layouts and names are Foundation's and Core Graphics' (NSUInteger is
// usize and CGFloat f64 on 64-bit).
unsafe impl Encode for NSRange {
    const ENCODING: Encoding = Encoding::Struct("_NSRange", &[usize::ENCODING, usize::ENCODING]);
}
unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

/// A view's context and the UTF-16 offsets of its text.
struct Snapshot {
    context: ImeContext,
    /// Bytes of the text the input method replaces with the composition it has just
    /// started over other text than the selection, until the next copy shows it.
    replacing: Option<Range<usize>>,
}

impl Snapshot {
    fn utf16(&self, byte: usize) -> usize {
        let text = &self.context.text;
        text[..byte.min(text.len())].encode_utf16().count()
    }

    fn utf16_range(&self, bytes: &Range<usize>) -> Range<usize> {
        self.utf16(bytes.start)..self.utf16(bytes.end)
    }

    /// The byte where UTF-16 offset `unit` falls (the start of its character),
    /// clamped to the text.
    fn byte(&self, unit: usize) -> usize {
        let mut units = 0;
        for (i, c) in self.context.text.char_indices() {
            if units + c.len_utf16() > unit {
                return i;
            }
            units += c.len_utf16();
        }
        self.context.text.len()
    }

    fn bytes(&self, units: Range<usize>) -> Range<usize> {
        self.byte(units.start)..self.byte(units.end)
    }

    fn len16(&self) -> usize {
        self.context.text.encode_utf16().count()
    }

    /// Where a composition starts or will start.
    fn anchor(&self) -> usize {
        let context = &self.context;
        let start = match (&context.composition, &self.replacing) {
            (Some(composition), _) => composition.start,
            (None, Some(replacing)) => replacing.start,
            (None, None) => context.selection.start,
        };
        self.utf16(start)
    }

    /// `markedRange` while winit holds a composition `composing` UTF-16 units long:
    /// winit knows its length, the copy where it starts.
    fn marked(&self, composing: Option<usize>) -> NSRange {
        match composing {
            Some(length) => NSRange::of(self.anchor()..self.anchor() + length),
            None => NSRange::NONE,
        }
    }

    /// `firstRectForCharacterRange:` for `range` (the caret when none): the box in
    /// window coordinates and the UTF-16 range it covers.
    fn first_rect(&self, range: NSRange) -> (Rect, Range<usize>) {
        let units = range.range().unwrap_or_else(|| {
            let selection = self.utf16_range(&self.context.selection);
            selection.start..selection.start
        });
        let bytes = self.bytes(units);
        let (rect, covered) = self.context.first_rect(bytes);
        (rect, self.utf16_range(&covered))
    }

    /// `selectedRange`, for a composition `composing` units long that winit holds.
    fn selected(&self, composing: Option<usize>) -> NSRange {
        match (self.context.composition.as_ref(), composing) {
            // winit has just begun a composition the copy has not seen yet.
            (None, Some(length)) => NSRange::of(self.anchor() + length..self.anchor() + length),
            _ => NSRange::of(self.utf16_range(&self.context.selection)),
        }
    }
}

thread_local! {
    /// By view.
    static SNAPSHOTS: RefCell<HashMap<usize, Snapshot>> = RefCell::default();
    /// The views [`sync`] has run for.
    static SYNCED: RefCell<std::collections::HashSet<usize>> = RefCell::default();
    /// A range the input method is about to replace, in bytes of the text shown, by
    /// view, until the composition it starts is handed on.
    static REPLACING: RefCell<HashMap<usize, Range<usize>>> = RefCell::default();
}

fn view_of(window: &Window) -> Option<usize> {
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr() as usize),
        _ => None,
    }
}

/// Keep the focused widget's context for `window`'s view; `None` when no widget
/// answers, and the original methods do.
pub fn sync(window: &Window, context: Option<ImeContext>) {
    let Some(view) = view_of(window) else {
        return;
    };
    SYNCED.with_borrow_mut(|synced| synced.insert(view));
    SNAPSHOTS.with_borrow_mut(|snapshots| match context {
        Some(context) => {
            snapshots.insert(
                view,
                Snapshot {
                    context,
                    replacing: None,
                },
            );
        }
        None => {
            snapshots.remove(&view);
        }
    });
}

/// Whether [`sync`] has run for `window` (since it was last [forgotten](forget)).
pub fn is_synced(window: &Window) -> bool {
    view_of(window).is_some_and(|view| SYNCED.with_borrow(|synced| synced.contains(&view)))
}

/// The range the input method replaces with the composition it has just started
/// (bytes of the text shown), if it is not the selection: send it as
/// [`noa_gpui_core::Event::ImeSelect`] before the composition.
pub fn take_replacement(window: &Window) -> Option<Range<usize>> {
    let view = view_of(window)?;
    REPLACING.with_borrow_mut(|replacing| replacing.remove(&view))
}

/// Forget a view's context when its window closes.
pub fn forget(window: &Window) {
    if let Some(view) = view_of(window) {
        SNAPSHOTS.with_borrow_mut(|s| s.remove(&view));
        SYNCED.with_borrow_mut(|s| s.remove(&view));
        REPLACING.with_borrow_mut(|r| r.remove(&view));
    }
}

fn with_snapshot<R>(view: *mut AnyObject, f: impl FnOnce(&Snapshot) -> R) -> Option<R> {
    SNAPSHOTS.with_borrow(|snapshots| snapshots.get(&(view as usize)).map(f))
}

struct Originals {
    selected_range: Imp,
    marked_range: Imp,
    substring: Imp,
    index_for_point: Imp,
    first_rect: Imp,
    set_marked_text: Imp,
}

static ORIGINALS: OnceLock<Originals> = OnceLock::new();

type RangeFn = unsafe extern "C-unwind" fn(*mut AnyObject, Sel) -> NSRange;
type SubstringFn =
    unsafe extern "C-unwind" fn(*mut AnyObject, Sel, NSRange, *mut NSRange) -> *mut AnyObject;
type IndexFn = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, CGPoint) -> usize;
type RectFn = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, NSRange, *mut NSRange) -> CGRect;
type SetMarkedFn =
    unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, NSRange, NSRange);

fn originals() -> &'static Originals {
    ORIGINALS
        .get()
        .expect("installed before the methods are replaced")
}

/// The original method of the view's class, as the function it is.
///
/// # Safety
/// `F` must be the method's signature.
unsafe fn original<F: Copy>(imp: Imp) -> F {
    // SAFETY: both are function pointers; the caller vouches for the signature.
    unsafe { std::mem::transmute_copy(&imp) }
}

unsafe extern "C-unwind" fn selected_range(this: *mut AnyObject, cmd: Sel) -> NSRange {
    let marked = unsafe { original::<RangeFn>(originals().marked_range)(this, cmd) };
    let answer = with_snapshot(this, |s| s.selected(marked.range().map(|r| r.len())));
    answer.unwrap_or_else(|| unsafe { original::<RangeFn>(originals().selected_range)(this, cmd) })
}

unsafe extern "C-unwind" fn marked_range(this: *mut AnyObject, cmd: Sel) -> NSRange {
    let marked = unsafe { original::<RangeFn>(originals().marked_range)(this, cmd) };
    let composing = marked.range().map(|r| r.len());
    with_snapshot(this, |s| s.marked(composing)).unwrap_or(marked)
}

unsafe extern "C-unwind" fn substring(
    this: *mut AnyObject,
    cmd: Sel,
    range: NSRange,
    actual: *mut NSRange,
) -> *mut AnyObject {
    let found = with_snapshot(this, |s| {
        let units = range.range()?;
        let len = s.len16();
        let units = units.start.min(len)..units.end.min(len);
        let bytes = s.bytes(units);
        let units = s.utf16_range(&bytes);
        Some((s.context.text[bytes].to_owned(), units))
    });
    let Some(found) = found else {
        return unsafe { original::<SubstringFn>(originals().substring)(this, cmd, range, actual) };
    };
    let Some((text, units)) = found else {
        return std::ptr::null_mut();
    };
    if !actual.is_null() {
        // SAFETY: AppKit passes a pointer to an NSRange to fill, or null.
        unsafe { *actual = NSRange::of(units) };
    }
    attributed_string(&text)
}

/// An autoreleased NSAttributedString of `text`.
fn attributed_string(text: &str) -> *mut AnyObject {
    // SAFETY: NSString's `initWithBytes:length:encoding:` copies the bytes, and
    // NSAttributedString's `initWithString:` copies the string; each `alloc` is balanced
    // by `release` or `autorelease`.
    unsafe {
        let string: *mut AnyObject = msg_send![objc2::class!(NSString), alloc];
        let string: *mut AnyObject = msg_send![
            string,
            initWithBytes: text.as_ptr().cast::<std::ffi::c_void>(),
            length: text.len(),
            encoding: UTF8,
        ];
        if string.is_null() {
            return std::ptr::null_mut();
        }
        let attributed: *mut AnyObject = msg_send![objc2::class!(NSAttributedString), alloc];
        let attributed: *mut AnyObject = msg_send![attributed, initWithString: string];
        let _: () = msg_send![string, release];
        if attributed.is_null() {
            return attributed;
        }
        msg_send![attributed, autorelease]
    }
}

unsafe extern "C-unwind" fn index_for_point(
    this: *mut AnyObject,
    cmd: Sel,
    point: CGPoint,
) -> usize {
    // SAFETY: `this` is winit's view, alive while AppKit asks it; NSWindow's
    // `convertPointFromScreen:` and NSView's `convertPoint:fromView:` take and return
    // points. The view is flipped, so its points are the window's logical pixels.
    let local = unsafe {
        let window: *mut AnyObject = msg_send![this, window];
        if window.is_null() {
            return NOT_FOUND;
        }
        let in_window: CGPoint = msg_send![window, convertPointFromScreen: point];
        let none: *mut AnyObject = std::ptr::null_mut();
        let local: CGPoint = msg_send![this, convertPoint: in_window, fromView: none];
        local
    };
    let found = with_snapshot(this, |s| {
        s.context
            .index_at(Point::new(local.x as f32, local.y as f32))
            .map_or(NOT_FOUND, |byte| s.utf16(byte))
    });
    found.unwrap_or_else(|| unsafe {
        original::<IndexFn>(originals().index_for_point)(this, cmd, point)
    })
}

unsafe extern "C-unwind" fn first_rect(
    this: *mut AnyObject,
    cmd: Sel,
    range: NSRange,
    actual: *mut NSRange,
) -> CGRect {
    let found = with_snapshot(this, |s| s.first_rect(range));
    let Some((rect, units)) = found else {
        return unsafe { original::<RectFn>(originals().first_rect)(this, cmd, range, actual) };
    };
    if !actual.is_null() {
        // SAFETY: AppKit passes a pointer to an NSRange to fill, or null.
        unsafe { *actual = NSRange::of(units) };
    }
    to_screen(this, rect)
}

/// `rect` in the view's (the window's logical) coordinates, on the screen.
fn to_screen(view: *mut AnyObject, rect: Rect) -> CGRect {
    let rect = CGRect {
        origin: CGPoint {
            x: f64::from(rect.x),
            y: f64::from(rect.y),
        },
        size: CGSize {
            width: f64::from(rect.width),
            height: f64::from(rect.height),
        },
    };
    // SAFETY: as in `index_for_point`; NSView's `convertRect:toView:` and NSWindow's
    // `convertRectToScreen:` take and return rectangles.
    unsafe {
        let window: *mut AnyObject = msg_send![view, window];
        if window.is_null() {
            return rect;
        }
        let none: *mut AnyObject = std::ptr::null_mut();
        let in_window: CGRect = msg_send![view, convertRect: rect, toView: none];
        msg_send![window, convertRectToScreen: in_window]
    }
}

unsafe extern "C-unwind" fn set_marked_text(
    this: *mut AnyObject,
    cmd: Sel,
    string: *mut AnyObject,
    selected: NSRange,
    replacement: NSRange,
) {
    // A composition that replaces other text than the selection: converting a word
    // again that the input method found around the caret.
    let replacing = replacement.range().and_then(|units| {
        SNAPSHOTS.with_borrow_mut(|snapshots| {
            let s = snapshots.get_mut(&(this as usize))?;
            let bytes = s.bytes(units);
            let context = &s.context;
            if context.composition.is_some() || bytes == context.selection {
                return None;
            }
            let shown = context.base + bytes.start..context.base + bytes.end;
            // Answers start the composition there until the next copy shows it.
            s.replacing = Some(bytes);
            Some(shown)
        })
    });
    if let Some(range) = replacing {
        REPLACING.with_borrow_mut(|r| r.insert(this as usize, range));
    }
    unsafe {
        original::<SetMarkedFn>(originals().set_marked_text)(
            this,
            cmd,
            string,
            selected,
            replacement,
        )
    }
}

/// Replace the methods of winit's view class. Call once a window exists (winit
/// registers the class when it makes the first view).
pub fn install() {
    if ORIGINALS.get().is_some() {
        return;
    }
    let Some(class) = AnyClass::get(c"WinitView") else {
        log::warn!("winit's view class is not registered: the IME queries stay unanswered");
        return;
    };
    let methods = [
        // SAFETY (all below): each function has the signature of the method it
        // replaces, which is what `Imp` is called with.
        (sel!(selectedRange), unsafe {
            std::mem::transmute::<RangeFn, Imp>(selected_range)
        }),
        (sel!(markedRange), unsafe {
            std::mem::transmute::<RangeFn, Imp>(marked_range)
        }),
        (
            sel!(attributedSubstringForProposedRange:actualRange:),
            unsafe { std::mem::transmute::<SubstringFn, Imp>(substring) },
        ),
        (sel!(characterIndexForPoint:), unsafe {
            std::mem::transmute::<IndexFn, Imp>(index_for_point)
        }),
        (sel!(firstRectForCharacterRange:actualRange:), unsafe {
            std::mem::transmute::<RectFn, Imp>(first_rect)
        }),
        (
            sel!(setMarkedText:selectedRange:replacementRange:),
            unsafe { std::mem::transmute::<SetMarkedFn, Imp>(set_marked_text) },
        ),
    ];
    let mut replaced = Vec::with_capacity(methods.len());
    for (selector, _) in &methods {
        let Some(method) = class.instance_method(*selector) else {
            log::warn!("winit's view has no {selector}: the IME queries stay unanswered");
            return;
        };
        replaced.push(method);
    }
    let previous: Vec<Imp> = replaced.iter().map(|m| m.implementation()).collect();
    ORIGINALS.get_or_init(|| Originals {
        selected_range: previous[0],
        marked_range: previous[1],
        substring: previous[2],
        index_for_point: previous[3],
        first_rect: previous[4],
        set_marked_text: previous[5],
    });
    for (method, (_, imp)) in replaced.into_iter().zip(methods) {
        // SAFETY: the replacement has the method's signature (see above), and the
        // original is kept to be called from it.
        unsafe { method.set_implementation(imp) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(text: &str, selection: Range<usize>) -> Snapshot {
        Snapshot {
            context: ImeContext {
                text: text.into(),
                selection,
                ..Default::default()
            },
            replacing: None,
        }
    }

    /// A range over two lines: the box is the first line's, and the range it covers
    /// says so, so the input method asks again for the rest.
    #[test]
    fn the_first_rect_covers_only_the_first_line() {
        let line = |x: f32, y: f32| Rect::new(x, y, 8.0, 16.0);
        let cluster = |bytes: Range<usize>, x: f32, y: f32| noa_gpui_text::Cluster {
            bytes,
            rect: line(x, y),
            rtl: false,
        };
        let mut s = snapshot("abcd", 0..0);
        s.context.clusters = vec![
            cluster(0..1, 0.0, 0.0),
            cluster(1..2, 8.0, 0.0),
            cluster(2..3, 0.0, 16.0),
            cluster(3..4, 8.0, 16.0),
        ];
        let (rect, covered) = s.first_rect(NSRange::of(1..4));
        assert_eq!(rect, line(8.0, 0.0));
        assert_eq!(covered, 1..2);
        let (rect, covered) = s.first_rect(NSRange::of(2..4));
        assert_eq!(rect, Rect::new(0.0, 16.0, 16.0, 16.0));
        assert_eq!(covered, 2..4);
    }

    /// The input method converts "abc" again with the caret at the end: until the
    /// next copy, its composition is where "abc" was, not at the caret.
    #[test]
    fn a_composition_over_other_text_starts_where_that_text_is() {
        let mut s = snapshot("abc def", 7..7);
        s.replacing = Some(0..3);
        assert_eq!(s.marked(Some(3)), NSRange::of(0..3));
        assert_eq!(s.selected(Some(3)), NSRange::of(3..3));
        // Without one, at the selection.
        s.replacing = None;
        assert_eq!(s.marked(Some(3)), NSRange::of(7..10));
        assert_eq!(s.marked(None), NSRange::NONE);
    }

    #[test]
    fn offsets_convert_between_bytes_and_utf16() {
        // "あ" is 3 bytes and 1 unit, "𝄞" 4 bytes and 2 units.
        let s = snapshot("aあ𝄞b", 4..8);
        assert_eq!(s.utf16(4), 2);
        assert_eq!(s.utf16(8), 4);
        assert_eq!(s.len16(), 5);
        assert_eq!(s.bytes(2..4), 4..8);
        // Inside a surrogate pair: the start of its character.
        assert_eq!(s.byte(3), 4);
        assert_eq!(s.byte(99), s.context.text.len());
        assert_eq!(s.anchor(), 2);
        assert_eq!(NSRange::of(2..4).range(), Some(2..4));
        assert_eq!(NSRange::NONE.range(), None);
    }
}
