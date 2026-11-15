use noa_gpui_core::widgets::{Button, Container, Label};
use noa_gpui_core::{Event, StyleMeta, StylePatch, StyleSheet, UiTree, WidgetId};
use noa_gpui_scene::Size;
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(400.0, 300.0);

const SHEET: &str = "
Button { padding: 4px }
Button.primary { background-color: #2255ff; color: #ffffff }
Container > Button:hover { border-width: 2px }
@media (min-width: 300px) { #save { border-radius: 9px } }
@media (max-width: 100px) { #save { border-radius: 1px } }
Label { color: #333333 }
";

struct Page {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
    save: WidgetId,
    label: WidgetId,
}

impl Page {
    fn new() -> Self {
        let mut tree = UiTree::new();
        tree.set_stylesheet(StyleSheet::parse(SHEET).unwrap());
        let root = tree.insert(None, Container::new(Default::default()));
        let save = tree.insert(Some(root), Button::new("Save"));
        tree.set_style_meta(save, StyleMeta::new().id("save").class("primary"));
        let label = tree.insert(Some(root), Label::new("Ready"));
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            ..Default::default()
        });
        tree.prepare(VIEWPORT, &mut text);
        Self {
            tree,
            text,
            root,
            save,
            label,
        }
    }
}

#[test]
fn inspecting_a_widget_names_it_and_lists_the_rules_that_shape_it() {
    let mut page = Page::new();
    let info = page.tree.inspect(page.save).unwrap();
    assert_eq!(info.selector(), "Button#save.primary");
    assert_eq!(info.rect, page.tree.window_rect(page.save).unwrap());
    assert_eq!(info.ancestors, ["Container"]);
    assert_eq!(info.pseudo_classes(), "");
    // Lowest precedence first: by specificity, then source order; the @media rule
    // whose condition fails is left out, and so is the :hover rule.
    let selectors: Vec<&str> = info.rules.iter().map(|r| r.selector.as_str()).collect();
    assert_eq!(selectors, ["Button", "Button.primary", "#save"]);
    assert_eq!(
        info.rules[1].declarations,
        ["background-color: #2255ff", "color: #ffffff"]
    );
    assert!(info.rules[2].in_media && !info.rules[0].in_media);

    // Hovering adds the state and the rule that depends on it.
    let rect = info.rect;
    page.tree.dispatch(
        Event::PointerMove {
            pos: noa_gpui_scene::Point::new(rect.x + 4.0, rect.y + 4.0),
        },
        &mut page.text,
    );
    let hovered = page.tree.inspect(page.save).unwrap();
    assert_eq!(hovered.pseudo_classes(), ":hover");
    assert!(
        hovered
            .rules
            .iter()
            .any(|r| r.selector == "Container > Button:hover")
    );
    let described = hovered.describe();
    assert!(
        described.starts_with("Button#save.primary:hover\n"),
        "{described}"
    );
    assert!(described.contains("in Container"), "{described}");
    assert!(
        described.ends_with("#save (@media) { border-radius: 9px }"),
        "{described}"
    );
}

#[test]
fn inline_styles_hidden_widgets_and_removed_ids_are_reported() {
    let mut page = Page::new();
    page.tree
        .set_inline_style(page.label, StylePatch::parse("font-size: 20px").unwrap());
    page.tree.set_hidden(page.label, true);
    page.tree.prepare(VIEWPORT, &mut page.text);
    let info = page.tree.inspect(page.label).unwrap();
    assert!(info.has_inline_style && info.hidden);
    assert!(info.describe().contains("(hidden)"));
    assert!(info.describe().ends_with("inline style"));

    page.tree.remove(page.label, &mut page.text);
    assert_eq!(page.tree.inspect(page.label), None);
}

#[test]
fn the_outline_lists_the_tree_in_order_with_frames() {
    let page = Page::new();
    let outline = page.tree.outline();
    let lines: Vec<&str> = outline.lines().collect();
    assert_eq!(lines.len(), 3, "{outline}");
    assert!(lines[0].starts_with("Container "), "{outline}");
    assert!(lines[1].starts_with("  Button#save.primary "), "{outline}");
    assert!(lines[2].starts_with("  Label "), "{outline}");
    let _ = page.root;
}
