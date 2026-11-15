//! The adapter's callbacks may run on another thread, so they only record what was asked
//! and wake the window; the requests are carried out on the next redraw.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use accesskit_winit::Adapter;
use noa_gpui_core::accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate,
};
use noa_gpui_text::TextStore;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::screen::Screen;

#[derive(Default)]
struct Shared {
    /// Assistive technology connected: the next redraw must send a full tree.
    activated: AtomicBool,
    actions: Mutex<Vec<ActionRequest>>,
}

impl Shared {
    fn run_actions(&self, screen: &mut dyn Screen, text: &mut TextStore) -> bool {
        let actions = std::mem::take(&mut *self.actions.lock().expect("not poisoned"));
        for request in &actions {
            screen.tree_mut().accessibility_action(request, text);
            for message in screen.tree_mut().take_messages() {
                screen.handle(message, text);
            }
            // Match Screen::dispatch: synchronize menus, selection and other
            // screen state only after the action's widget messages were handled.
            screen.after_input();
        }
        !actions.is_empty()
    }
}

struct Handler {
    shared: Arc<Shared>,
    window: Arc<Window>,
}

impl ActivationHandler for Handler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.shared.activated.store(true, Ordering::Release);
        self.window.request_redraw();
        None
    }
}

impl ActionHandler for Handler {
    fn do_action(&mut self, request: ActionRequest) {
        self.shared
            .actions
            .lock()
            .expect("not poisoned")
            .push(request);
        self.window.request_redraw();
    }
}

impl DeactivationHandler for Handler {
    fn deactivate_accessibility(&mut self) {}
}

pub struct Accessibility {
    adapter: Adapter,
    shared: Arc<Shared>,
}

impl Accessibility {
    /// Attach to `window`, which must not have been shown yet; it is shown afterwards.
    pub fn attach(event_loop: &ActiveEventLoop, window: &Arc<Window>) -> Self {
        let shared = Arc::new(Shared::default());
        let handler = || Handler {
            shared: shared.clone(),
            window: window.clone(),
        };
        let adapter =
            Adapter::with_direct_handlers(event_loop, window, handler(), handler(), handler());
        window.set_visible(true);
        Self { adapter, shared }
    }

    /// Let the adapter see every window event first.
    pub fn process_event(&mut self, window: &Window, event: &WindowEvent) {
        self.adapter.process_event(window, event);
    }

    /// Carry out queued requests from assistive technology. Returns whether there were
    /// any (the tree then needs a redraw).
    pub fn run_actions(&mut self, screen: &mut dyn Screen, text: &mut TextStore) -> bool {
        self.shared.run_actions(screen, text)
    }

    /// Send the tree if something changed or assistive technology just connected.
    pub fn update(&mut self, screen: &dyn Screen, changed: bool, scale_factor: f64) {
        let activated = self.shared.activated.swap(false, Ordering::AcqRel);
        if changed || activated {
            self.adapter
                .update_if_active(|| screen.tree().accessibility_update(scale_factor));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noa_gpui_core::accesskit::{Action, TreeId};
    use noa_gpui_core::widgets::{Button, ButtonClicked, Container, Label};
    use noa_gpui_core::{Message, UiTree, WidgetId, access_id};

    struct CounterScreen {
        tree: UiTree,
        button: WidgetId,
        label: WidgetId,
        clicks: usize,
        synchronized: Vec<usize>,
    }

    impl CounterScreen {
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
                synchronized: Vec::new(),
            }
        }

        fn request(&self, action: Action) -> ActionRequest {
            ActionRequest {
                action,
                target_tree: TreeId::ROOT,
                target_node: access_id(self.button),
                data: None,
            }
        }
    }

    impl Screen for CounterScreen {
        fn tree(&self) -> &UiTree {
            &self.tree
        }

        fn tree_mut(&mut self) -> &mut UiTree {
            &mut self.tree
        }

        fn handle(&mut self, message: Message, _text: &mut TextStore) {
            if message.source == self.button && message.downcast_ref::<ButtonClicked>().is_some() {
                self.clicks += 1;
            }
        }

        fn after_input(&mut self) {
            self.synchronized.push(self.clicks);
            self.tree.update::<Label, _>(self.label, |label, cx| {
                label.set_text(cx, self.clicks.to_string());
            });
        }
    }

    #[test]
    fn accessibility_actions_synchronize_the_screen_after_handling_each_message() {
        let shared = Shared::default();
        let mut screen = CounterScreen::new();
        let mut text = TextStore::new(Default::default());
        *shared.actions.lock().unwrap() =
            vec![screen.request(Action::Click), screen.request(Action::Click)];

        assert!(shared.run_actions(&mut screen, &mut text));
        assert_eq!(screen.clicks, 2, "both button messages were delivered");
        assert_eq!(screen.synchronized, [1, 2]);
        assert_eq!(screen.tree.get::<Label>(screen.label).unwrap().text(), "2");

        assert!(!shared.run_actions(&mut screen, &mut text));
        assert_eq!(screen.synchronized, [1, 2], "an empty queue is not input");
    }

    #[test]
    fn accessibility_focus_synchronizes_even_without_a_widget_message() {
        let shared = Shared::default();
        let mut screen = CounterScreen::new();
        let mut text = TextStore::new(Default::default());
        shared
            .actions
            .lock()
            .unwrap()
            .push(screen.request(Action::Focus));

        assert!(shared.run_actions(&mut screen, &mut text));
        assert_eq!(screen.tree.focused(), Some(screen.button));
        assert_eq!(screen.clicks, 0);
        assert_eq!(screen.synchronized, [0]);
    }
}
