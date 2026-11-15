//! Renders one small settings panel in every Design x Appearance, with noa-gpui's
//! own Headless renderer, for the landing page's theme gallery.
use ui_platform::Headless;
use ui_platform::ui_core::style::{AlignItems, JustifyContent};
use ui_platform::ui_core::widgets::{
    Button, Checkbox, Container, Label, ProgressBar, SegmentMode, SegmentedControl, Slider,
    Switch, TextField,
};
use ui_platform::ui_core::{Appearance, Design, Role, Theme, UiTree};
use ui_platform::ui_text::{FontOptions, TextStore};

fn panel(theme: Theme) -> UiTree {
    let mut t = UiTree::new();
    t.set_theme(theme);
    let root = t.insert(None, Container::column().padding(22.0).gap(14.0).background(Role::Background).width(380.0));
    t.insert(Some(root), Label::new("Export settings").font_size(17.0).weight(600));
    t.insert(Some(root), SegmentedControl::new(["Image", "Video", "Audio"], SegmentMode::SelectOne));
    t.insert(Some(root), TextField::new("File name", 336.0));
    let row = t.insert(Some(root), Container::row().gap(10.0).align_items(AlignItems::CENTER).justify_content(JustifyContent::SPACE_BETWEEN).width(336.0));
    t.insert(Some(row), Label::new("Include metadata"));
    t.insert(Some(row), Switch::new(true));
    t.insert(Some(root), Checkbox::new("Open folder when done").checked(true));
    t.insert(Some(root), Slider::new(0.0, 100.0, 64.0));
    t.insert(Some(root), ProgressBar::new(336.0).value(0.42, 1.0));
    let buttons = t.insert(Some(root), Container::row().gap(8.0).justify_content(JustifyContent::FLEX_END).width(336.0));
    t.insert(Some(buttons), Button::new("Cancel"));
    t.insert(Some(buttons), Button::new("Export").default_button(true));
    t
}

fn main() {
    let out = std::env::args().nth(1).expect("out dir");
    let mut headless = Headless::new_or_software();
    let mut text = TextStore::new(FontOptions::default());
    for design in Design::ALL {
        for (app, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
            let mut tree = panel(Theme::new(design, app));
            let img = headless.render_tree(&mut tree, &mut text, (380, 330), 2.0).unwrap();
            let path = format!("{out}/{}-{name}.png", design.name().to_lowercase().replace(' ', "-"));
            img.save_png(&path).unwrap();
            println!("wrote {path}");
        }
    }
}
