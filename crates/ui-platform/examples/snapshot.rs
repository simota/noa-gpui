//! Draws a widget tree without a window and writes it to a PNG file, on the GPU or, where
//! there is no GPU adapter, on the CPU.
//!
//! ```text
//! cargo run -p ui-platform --example snapshot --features native -- snapshot.png
//! ```

use ui_platform::Headless;
use ui_platform::ui_core::style::{AlignItems, FlexDirection, JustifyContent, Size, percent};
use ui_platform::ui_core::widgets::{Button, Container, Label};
use ui_platform::ui_core::{Role, Style, UiTree};
use ui_platform::ui_text::{FontOptions, TextStore};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "snapshot.png".into());
    let mut tree = UiTree::new();
    let root = tree.insert(
        None,
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            size: Size {
                width: percent(1.0),
                height: percent(1.0),
            },
            ..Default::default()
        })
        .background(Role::Background),
    );
    tree.insert(Some(root), Label::new("Rendered without a window"));
    tree.insert(Some(root), Button::new("OK"));

    let mut headless = Headless::new_or_software();
    let mut text = TextStore::new(FontOptions::default());
    let image = headless
        .render_tree(&mut tree, &mut text, (480, 320), 2.0)
        .unwrap_or_else(|e| panic!("{e}"));
    image
        .save_png(&path)
        .unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote {path} ({}x{})", image.width, image.height);
}
