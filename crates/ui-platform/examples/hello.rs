//! The smallest application: a window with a button that counts its clicks.
//!
//! ```text
//! cargo run -p noa-gpui-platform --example hello --features native
//! ```

use noa_gpui_platform::noa_gpui_core::style::{
    AlignItems, FlexDirection, JustifyContent, Size, length, percent,
};
use noa_gpui_platform::noa_gpui_core::widgets::{Button, ButtonClicked, Container, Label};
use noa_gpui_platform::noa_gpui_core::{Message, Role, Style, UiTree, WidgetId};
use noa_gpui_platform::noa_gpui_text::TextStore;
use noa_gpui_platform::{App, AppOptions, Screen};

struct Hello {
    tree: UiTree,
    button: WidgetId,
    label: WidgetId,
    clicks: u32,
}

impl Hello {
    fn new() -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::CENTER,
                justify_content: JustifyContent::CENTER,
                gap: Size {
                    width: length(0.0),
                    height: length(12.0),
                },
                size: Size {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .background(Role::Background),
        );
        let label = tree.insert(Some(root), Label::new("Clicked 0 times"));
        let button = tree.insert(Some(root), Button::new("Click me"));
        Self {
            tree,
            button,
            label,
            clicks: 0,
        }
    }
}

impl Screen for Hello {
    fn tree(&self) -> &UiTree {
        &self.tree
    }

    fn tree_mut(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    fn handle(&mut self, message: Message, _text: &mut TextStore) {
        if message.source == self.button && message.downcast_ref::<ButtonClicked>().is_some() {
            self.clicks += 1;
            let text = format!("Clicked {} times", self.clicks);
            self.tree
                .update::<Label, _>(self.label, |l, cx| l.set_text(cx, text));
        }
    }
}

fn main() {
    let options = AppOptions {
        size: (480.0, 320.0),
        ..AppOptions::new("Hello")
    };
    App::new(options, |_| Box::new(Hello::new()))
        .run()
        .expect("run event loop");
}
