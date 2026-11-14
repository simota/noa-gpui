//! winit registers its own application delegate when the event loop is built and
//! panics if another one replaces it, so instead of becoming the delegate this adds
//! `applicationShouldHandleReopen:hasVisibleWindows:` to winit's delegate class
//! (winit does not implement it).

use std::ffi::CString;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::encode::Encode;
use objc2::runtime::{AnyObject, Bool, Imp, Sel};
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::NSApplication;

use crate::wake::Waker;

static REOPENED: AtomicBool = AtomicBool::new(false);
static WAKER: OnceLock<Waker> = OnceLock::new();

unsafe extern "C-unwind" fn should_handle_reopen(
    _this: *mut AnyObject,
    _cmd: Sel,
    _sender: *mut AnyObject,
    _visible: Bool,
) -> Bool {
    REOPENED.store(true, Ordering::Release);
    if let Some(waker) = WAKER.get() {
        waker.nudge();
    }
    // AppKit still restores minimized windows; the app shows hidden ones.
    Bool::YES
}

/// Wake the event loop through `waker` on a reopen. Call after the event loop is
/// built (winit has registered its application delegate by then).
pub fn install(waker: &Waker) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    WAKER.get_or_init(|| waker.clone());
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else {
        return;
    };
    let selector = sel!(applicationShouldHandleReopen:hasVisibleWindows:);
    let object: &AnyObject = delegate.as_ref();
    let class = object.class();
    if class.instance_method(selector).is_some() {
        return;
    }
    let types = CString::new(format!("{0}@:@{0}", Bool::ENCODING)).expect("no NUL");
    // SAFETY: `should_handle_reopen` has the C signature the type string describes
    // (BOOL, self, _cmd, NSApplication *, BOOL), and the class is winit's delegate
    // class, which lives for the whole process.
    unsafe {
        let imp: Imp = std::mem::transmute(
            should_handle_reopen
                as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, Bool) -> Bool,
        );
        objc2::ffi::class_addMethod(class as *const _ as *mut _, selector, imp, types.as_ptr());
    }
    // NSApplication caches which methods its delegate implements when it is set;
    // setting the same delegate again refreshes that.
    app.setDelegate(Some(&delegate));
}

/// Whether the Dock icon was clicked since the last call.
pub fn take() -> bool {
    REOPENED.swap(false, Ordering::AcqRel)
}
