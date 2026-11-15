#[cfg(all(feature = "native", target_os = "macos"))]
use noa_gpui_core::widgets::MenuBarMenu;
#[cfg(all(
    feature = "native",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
use noa_gpui_core::widgets::MenuItem;
use noa_gpui_core::{Appearance, Event, Message, UiTree};
use noa_gpui_scene::{Color, DisplayList, Rect, Size};
use noa_gpui_text::TextStore;

use crate::app::FrameReport;

/// A window a screen wants opened (desktop only).
#[cfg(feature = "native")]
pub struct WindowRequest {
    pub title: String,
    /// Logical size.
    pub size: (f64, f64),
    pub screen: Box<dyn Screen>,
    /// A panel: stays above the app's other windows and is not resizable.
    pub floating: bool,
}

#[cfg(feature = "native")]
pub struct Detached {
    pub title: String,
    pub content: Vec<noa_gpui_core::Subtree>,
}

/// An open or save panel a screen wants shown; the desktop app shows the OS's own
/// (`NSOpenPanel` / `NSSavePanel` on macOS).
#[cfg(feature = "native")]
#[derive(Debug, Clone)]
pub struct FileDialogRequest {
    pub save: bool,
    pub title: String,
    /// The suggested name (save panels).
    pub file_name: String,
    pub filters: Vec<(String, Vec<String>)>,
}

#[cfg(feature = "native")]
#[derive(Debug, Clone, PartialEq)]
pub enum FileDialogResult {
    Opened(Vec<std::path::PathBuf>),
    Saved(std::path::PathBuf),
    Cancelled,
}

pub trait Screen {
    fn tree(&self) -> &UiTree;
    fn tree_mut(&mut self) -> &mut UiTree;

    /// React to a message emitted by a widget during [`Screen::dispatch`].
    fn handle(&mut self, message: Message, text: &mut TextStore);

    /// The OS switched between light and dark. Keeps the current design and accent.
    fn set_system_appearance(&mut self, appearance: Appearance) {
        let theme = self.tree().theme().clone().with_appearance(appearance);
        self.tree_mut().set_theme(theme);
    }

    /// Called for the main window's screen at the start of every frame, before
    /// animations run and the tree is laid out.
    fn before_frame(&mut self, _text: &mut TextStore) {}

    fn frame_presented(&mut self, _report: &FrameReport<'_>) {}

    /// Whether the main window renders every frame, even when nothing changed (a
    /// benchmark).
    fn continuous(&self) -> bool {
        false
    }

    /// Clear color behind the tree: the theme's background.
    fn background(&self) -> Color {
        self.tree().theme().background
    }

    /// Windows to open, collected since the last call.
    #[cfg(feature = "native")]
    fn take_window_requests(&mut self) -> Vec<WindowRequest> {
        Vec::new()
    }

    /// Open and save panels to show, collected since the last call.
    #[cfg(feature = "native")]
    fn take_file_dialogs(&mut self) -> Vec<FileDialogRequest> {
        Vec::new()
    }

    #[cfg(feature = "native")]
    fn file_dialog_finished(&mut self, _result: FileDialogResult, _text: &mut TextStore) {}

    /// Content this screen (in a secondary window) gives back to the main window.
    #[cfg(feature = "native")]
    fn take_detached(&mut self) -> Option<Detached> {
        None
    }

    #[cfg(feature = "native")]
    fn adopt(&mut self, _detached: Detached, _text: &mut TextStore) {}

    /// The user closed the window showing this screen (its close button). Content to
    /// keep goes to [`Screen::take_detached`], which is read before the window closes.
    #[cfg(feature = "native")]
    fn close_requested(&mut self) {}

    /// A [`crate::Waker`] was woken (from any thread) since the last call. Runs on the
    /// main thread, for the main screen. The window redraws afterwards.
    fn woken(&mut self, _text: &mut TextStore) {}

    /// Desktop, main screen: `Some(false)` hides the main window, `Some(true)` shows it
    /// again (and brings it to the front). Read after each event; return each request
    /// once. With [`crate::AppOptions::keep_running`] the app keeps running while the
    /// window is hidden, so a status menu item can show it again. Wayland does not let
    /// applications hide their windows.
    fn take_main_window_visibility(&mut self) -> Option<bool> {
        None
    }

    /// Desktop, main screen: the main window was hidden (`false`, also when the user
    /// closed it with [`crate::AppOptions::keep_running`]) or shown again (`true`).
    fn main_window_visibility_changed(&mut self, _visible: bool, _text: &mut TextStore) {}

    /// macOS, main screen: the user clicked the app's Dock icon. A hidden main window
    /// has been shown again already.
    #[cfg(all(feature = "native", target_os = "macos"))]
    fn reopened(&mut self, _text: &mut TextStore) {}

    /// Whether the window showing this screen should close. Closing the main window
    /// ends the application.
    fn wants_close(&self) -> bool {
        false
    }

    /// The application's menus. The desktop app on macOS puts them in the system menu
    /// bar (and calls [`Screen::use_system_menu_bar`]); elsewhere the screen draws them.
    #[cfg(all(feature = "native", target_os = "macos"))]
    fn app_menus(&self) -> Vec<MenuBarMenu> {
        Vec::new()
    }

    /// An item of [`Screen::app_menus`] was chosen from the system menu bar: `item` is
    /// its depth-first index in menu `menu`.
    #[cfg(all(feature = "native", target_os = "macos"))]
    fn app_menu_chosen(&mut self, _menu: usize, _item: usize, _text: &mut TextStore) {}

    /// The system menu bar shows [`Screen::app_menus`]: hide any menu bar in the window.
    #[cfg(all(feature = "native", target_os = "macos"))]
    fn use_system_menu_bar(&mut self) {}

    /// Whether the app menus or the menu bar extra changed since the last call, so the
    /// system's copies must be rebuilt.
    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn take_system_menus_changed(&mut self) -> bool {
        false
    }

    /// The menu of the menu bar extra (macOS) or tray icon (Windows; Linux desktops with
    /// a StatusNotifierItem host), or `None` for no icon.
    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn status_menu(&self) -> Option<Vec<MenuItem>> {
        None
    }

    /// An item of [`Screen::status_menu`] was chosen (depth-first index).
    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn status_menu_chosen(&mut self, _item: usize, _text: &mut TextStore) {}

    /// Called after every dispatched event, once its messages are handled.
    fn after_input(&mut self) {}

    fn dispatch(&mut self, event: Event, text: &mut TextStore) {
        self.tree_mut().dispatch(event, text);
        for message in self.tree_mut().take_messages() {
            self.handle(message, text);
        }
        self.after_input();
    }

    /// Deliver animation frames at `now` (seconds), handling any messages they emit.
    fn animate(&mut self, now: f64, text: &mut TextStore) {
        self.tree_mut().animate(now, text);
        for message in self.tree_mut().take_messages() {
            self.handle(message, text);
        }
    }

    fn needs_update(&self, viewport: Size) -> bool {
        self.tree().needs_update(viewport) || self.tree().wants_animation_frame()
    }

    fn prepare(&mut self, viewport: Size, text: &mut TextStore) -> bool {
        self.tree_mut().prepare(viewport, text).is_some()
    }

    fn display_list(&self) -> &DisplayList {
        self.tree().display_list()
    }

    /// Caret rectangle of the focused text field, in window coordinates.
    fn text_input_area(&self) -> Option<Rect> {
        self.tree().text_input_area()
    }

    fn widget_count(&self) -> usize {
        self.tree().len()
    }
}

/// Carry out requests from assistive technology (AccessKit actions) on `screen`,
/// handling the messages each one emits. Returns whether there were any.
#[cfg(any(feature = "native", target_arch = "wasm32"))]
pub(crate) fn run_accessibility_actions(
    screen: &mut dyn Screen,
    text: &mut TextStore,
    actions: &[noa_gpui_core::accesskit::ActionRequest],
) -> bool {
    for request in actions {
        screen.tree_mut().accessibility_action(request, text);
        for message in screen.tree_mut().take_messages() {
            screen.handle(message, text);
        }
        // Match Screen::dispatch: synchronize menus, selection and other screen
        // state only after the action's widget messages were handled.
        screen.after_input();
    }
    !actions.is_empty()
}
