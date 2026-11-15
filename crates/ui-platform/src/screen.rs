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
    /// The title bar (macOS).
    pub titlebar: crate::Titlebar,
    /// The smallest logical size the window can be resized to.
    pub min_size: Option<(f64, f64)>,
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

/// A move or resize of the window the OS carries out, following the pointer until the
/// button is released: for a title bar or border the screen draws itself (see
/// [`crate::AppOptions::decorations`]).
#[cfg(feature = "native")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowDrag {
    Move,
    /// From the given edge or corner.
    Resize(winit::window::ResizeDirection),
}

/// A question a screen wants asked in the OS's own alert (`NSAlert` on macOS, a task
/// dialog on Windows, zenity on Linux): a confirm and a cancel button, such as
/// "Delete" and "Cancel".
///
/// Windows shows OK and Cancel in place of the labels.
#[cfg(feature = "native")]
#[derive(Debug, Clone, PartialEq)]
pub struct AlertRequest {
    pub level: AlertLevel,
    pub title: String,
    pub message: String,
    pub confirm: String,
    pub cancel: String,
}

/// The icon an [`AlertRequest`] shows.
#[cfg(feature = "native")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertLevel {
    #[default]
    Info,
    Warning,
    Error,
}

#[cfg(feature = "native")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertResult {
    Confirmed,
    /// The cancel button, Escape, or closing the alert.
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

    /// Window moves and resizes to start, collected since the last call. Read right
    /// after each input event, so ask for one while handling a pointer press: the OS
    /// starts it only while the button is down. The OS then takes the pointer, so the
    /// widget pressed may not see the release.
    #[cfg(feature = "native")]
    fn take_window_drags(&mut self) -> Vec<WindowDrag> {
        Vec::new()
    }

    /// Open and save panels to show, collected since the last call.
    #[cfg(feature = "native")]
    fn take_file_dialogs(&mut self) -> Vec<FileDialogRequest> {
        Vec::new()
    }

    #[cfg(feature = "native")]
    fn file_dialog_finished(&mut self, _result: FileDialogResult, _text: &mut TextStore) {}

    /// Alerts to show, collected since the last call. They are shown one after the
    /// other, each answered through [`Screen::alert_finished`] before the next.
    #[cfg(feature = "native")]
    fn take_alerts(&mut self) -> Vec<AlertRequest> {
        Vec::new()
    }

    #[cfg(feature = "native")]
    fn alert_finished(&mut self, _result: AlertResult, _text: &mut TextStore) {}

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

    /// The focused text as the input method may ask about it, in window coordinates.
    fn ime_context(&self, text: &TextStore) -> Option<noa_gpui_core::ImeContext> {
        self.tree().ime_context(text)
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

/// Seconds between the frames [`replay`] and [`advance`] draw while something moves.
const FRAME: f64 = 1.0 / 60.0;

/// Play `recording` back into `screen` laid out at `size` logical points, as the app
/// would: between steps, animations get a frame every 1/60 s and timers fire when
/// due, so each input meets the layout a window would have shown at that moment; each
/// step's input is then dispatched at its time (messages handled). Draw the result
/// with [`crate::Headless::render_screen`], after [`advance`] if animations started by
/// the last steps should finish first.
pub fn replay(
    screen: &mut dyn Screen,
    recording: &noa_gpui_core::recording::Recording,
    text: &mut TextStore,
    size: Size,
) {
    screen.prepare(size, text);
    let Some(first) = recording.steps.first() else {
        return;
    };
    let mut now = first.at;
    for step in &recording.steps {
        advance_from(screen, text, size, now, step.at);
        now = step.at;
        screen.tree_mut().set_now(now);
        screen.dispatch(step.event.clone(), text);
        frame(screen, text, size, now);
    }
}

/// Let `seconds` pass on `screen`'s clock as in a window left alone: animations get a
/// frame every 1/60 s and timers fire when due.
pub fn advance(screen: &mut dyn Screen, text: &mut TextStore, size: Size, seconds: f64) {
    let now = screen.tree().now();
    advance_from(screen, text, size, now, now + seconds);
}

fn advance_from(screen: &mut dyn Screen, text: &mut TextStore, size: Size, from: f64, until: f64) {
    let mut now = from;
    while now < until {
        let tree = screen.tree();
        // While nothing moves, go straight to the next due timer (or the end); a frame
        // is the shortest step, as in a window.
        let next = if tree.wants_animation_frame() {
            now + FRAME
        } else {
            tree.next_timer().map_or(until, |due| due.max(now + FRAME))
        };
        now = next.min(until);
        screen.tree_mut().set_now(now);
        frame(screen, text, size, now);
    }
}

/// What the app does each frame: animation frames and due timers, then layout.
fn frame(screen: &mut dyn Screen, text: &mut TextStore, size: Size, now: f64) {
    if crate::app::wants_frame(screen.tree(), now) {
        screen.animate(now, text);
    }
    screen.prepare(size, text);
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use noa_gpui_core::widgets::{Button, ButtonClicked, Container, Label};
    use noa_gpui_core::{Event, Message, PointerButton, UiTree, WidgetId};
    use noa_gpui_render_wgpu::{Tolerance, check_golden};
    use noa_gpui_scene::{Point, Size};
    use noa_gpui_text::TextStore;

    use super::{Screen, advance, replay};
    use crate::Headless;

    const SIZE: Size = Size::new(240.0, 120.0);

    struct Counter {
        tree: UiTree,
        button: WidgetId,
        label: WidgetId,
        clicks: usize,
    }

    impl Counter {
        fn with_tooltip() -> Self {
            let mut tree = UiTree::new();
            let root = tree.insert(None, Container::new(Default::default()));
            let button = tree.insert(Some(root), Button::new("Count").tooltip("Adds one"));
            let label = tree.insert(Some(root), Label::new("0"));
            Self {
                tree,
                button,
                label,
                clicks: 0,
            }
        }

        /// A screen around `tree`; `id` stands in for the button and label.
        fn from_tree(tree: UiTree, id: WidgetId) -> Self {
            Self {
                tree,
                button: id,
                label: id,
                clicks: 0,
            }
        }

        fn new() -> Self {
            let mut tree = UiTree::new();
            let root = tree.insert(None, Container::new(Default::default()));
            let button = tree.insert(Some(root), Button::new("Count"));
            let label = tree.insert(Some(root), Label::new("0"));
            Self {
                tree,
                button,
                label,
                clicks: 0,
            }
        }
    }

    impl Screen for Counter {
        fn tree(&self) -> &UiTree {
            &self.tree
        }

        fn tree_mut(&mut self) -> &mut UiTree {
            &mut self.tree
        }

        fn handle(&mut self, message: Message, _text: &mut TextStore) {
            if message.source == self.button && message.downcast_ref::<ButtonClicked>().is_some() {
                self.clicks += 1;
                let clicks = self.clicks.to_string();
                self.tree
                    .update::<Label, _>(self.label, |l, cx| l.set_text(cx, clicks));
            }
        }
    }

    #[test]
    fn a_replayed_session_draws_what_the_recorded_one_did() {
        let mut text = TextStore::new(Default::default());
        let mut original = Counter::new();
        original.prepare(SIZE, &mut text);
        original.tree.start_recording();
        let r = original.tree.window_rect(original.button).unwrap();
        let pos = Point::new(r.x + 8.0, r.y + 8.0);
        let button = PointerButton::Primary;
        for (i, at) in [0.0, 1.0, 2.0].into_iter().enumerate() {
            original.tree.set_now(at);
            original.dispatch(Event::PointerMove { pos }, &mut text);
            original.dispatch(Event::PointerDown { pos, button }, &mut text);
            original.dispatch(Event::PointerUp { pos, button }, &mut text);
            assert_eq!(original.clicks, i + 1);
            original.prepare(SIZE, &mut text);
        }
        let recording = original.tree.stop_recording();

        let mut replayed = Counter::new();
        replay(&mut replayed, &recording, &mut text, SIZE);
        assert_eq!(replayed.clicks, 3);

        // Compare once hover and press animations have finished on both.
        for screen in [&mut original as &mut dyn Screen, &mut replayed] {
            advance(screen, &mut text, SIZE, 2.0);
        }

        let mut headless = Headless::software();
        let size = (SIZE.width as u32, SIZE.height as u32);
        let expected = headless
            .render_screen(&mut original, &mut text, size, 1.0)
            .unwrap();
        let actual = headless
            .render_screen(&mut replayed, &mut text, size, 1.0)
            .unwrap();
        let dir = std::env::temp_dir().join(format!("noa-gpui-replay-{}", std::process::id()));
        let golden = dir.join("counter.png");
        std::fs::create_dir_all(&dir).unwrap();
        expected.save_png(&golden).unwrap();
        let result = check_golden(&actual, &golden, Tolerance::default());
        if result.is_ok() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        result.expect("the replayed screen draws as the recorded one");
    }

    #[test]
    fn timers_fire_between_steps_without_running_animations() {
        let mut text = TextStore::new(Default::default());
        let mut screen = Counter::new();
        screen.prepare(SIZE, &mut text);
        let r = screen.tree.window_rect(screen.button).unwrap();
        let pos = Point::new(r.x + 8.0, r.y + 8.0);
        // The button's tooltip opens on a timer after the pointer rests on it; the next
        // step comes long after.
        let recording = noa_gpui_core::recording::Recording {
            steps: vec![
                noa_gpui_core::recording::Step {
                    at: 0.0,
                    event: Event::PointerMove { pos },
                },
                noa_gpui_core::recording::Step {
                    at: 5.0,
                    event: Event::ModifiersChanged(Default::default()),
                },
            ],
        };
        let mut tipped = Counter::with_tooltip();
        replay(&mut tipped, &recording, &mut text, SIZE);
        assert!(
            tipped.tree.tooltip_overlay().is_some(),
            "the tooltip opened"
        );
    }

    #[test]
    fn scrolling_momentum_runs_between_steps_as_in_a_window() {
        use noa_gpui_core::widgets::ScrollView;

        let mut text = TextStore::new(Default::default());
        let build = || {
            let mut tree = UiTree::new();
            let root = tree.insert(None, Container::new(Default::default()));
            let scroll = tree.insert(
                Some(root),
                ScrollView::new(noa_gpui_core::Style {
                    size: noa_gpui_core::style::Size {
                        width: noa_gpui_core::style::length(200.0),
                        height: noa_gpui_core::style::length(100.0),
                    },
                    ..Default::default()
                }),
            );
            let column = tree.insert(
                Some(scroll),
                Container::new(noa_gpui_core::Style {
                    flex_direction: noa_gpui_core::style::FlexDirection::Column,
                    ..Default::default()
                }),
            );
            for i in 0..60 {
                tree.insert(Some(column), Label::new(format!("Row {i}")));
            }
            (tree, scroll)
        };
        let wheel = |at: f64| noa_gpui_core::recording::Step {
            at,
            event: Event::Wheel {
                pos: Point::new(50.0, 50.0),
                delta: Point::new(0.0, -40.0),
            },
        };
        // A flick, then a pause long enough for any momentum to stop.
        let recording = noa_gpui_core::recording::Recording {
            steps: vec![
                wheel(0.0),
                wheel(0.016),
                wheel(0.032),
                noa_gpui_core::recording::Step {
                    at: 3.0,
                    event: Event::ModifiersChanged(Default::default()),
                },
            ],
        };

        // What a window shows: frames every 1/60 s from the flick until the pause ends.
        let (tree, scroll) = build();
        let mut window = Counter::from_tree(tree, scroll);
        window.prepare(SIZE, &mut text);
        let mut now = 0.0;
        for step in &recording.steps {
            while now + 1.0 / 60.0 < step.at {
                now += 1.0 / 60.0;
                window.tree.set_now(now);
                window.animate(now, &mut text);
                window.prepare(SIZE, &mut text);
            }
            now = step.at;
            window.tree.set_now(now);
            window.dispatch(step.event.clone(), &mut text);
            window.animate(now, &mut text);
            window.prepare(SIZE, &mut text);
        }

        let (tree, replayed_scroll) = build();
        let mut replayed = Counter::from_tree(tree, replayed_scroll);
        replay(&mut replayed, &recording, &mut text, SIZE);
        let offset = |tree: &UiTree, id| tree.get::<ScrollView>(id).unwrap().offset();
        assert!(offset(&window.tree, scroll) > 0.0, "the flick scrolled");
        assert_eq!(
            offset(&replayed.tree, replayed_scroll),
            offset(&window.tree, scroll)
        );
    }
}
