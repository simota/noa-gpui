use noa_gpui_layout::style::*;
use noa_gpui_layout::{LayoutTree, Style};
use noa_gpui_scene::Size as Viewport;

#[test]
fn stretched_wrapping_items_depend_on_their_contents() {
    let mut tree: LayoutTree<u32> = LayoutTree::new();
    let root = tree.add(
        Style {
            size: Size {
                width: length(100.0),
                height: length(100.0),
            },
            flex_wrap: FlexWrap::Wrap,
            ..Default::default()
        },
        None,
    );
    let item_style = Style {
        size: Size {
            width: length(60.0),
            height: auto(),
        },
        flex_shrink: 0.0,
        ..Default::default()
    };
    let first = tree.add(item_style.clone(), None);
    let second = tree.add(item_style, None);
    let content = tree.add(Style::default(), Some(0));
    let other_content = tree.add(Style::default(), Some(1));
    tree.set_children(root, &[first, second]);
    tree.set_children(first, &[content]);
    tree.set_children(second, &[other_content]);
    let viewport = Viewport::new(100.0, 100.0);
    tree.compute(root, viewport, |_, _| Viewport::new(10.0, 10.0));
    let before = tree.layout(first).rect.height;
    let sibling_before = tree.layout(second).rect;

    tree.mark_dirty(content);
    tree.compute(root, viewport, |key, _| {
        Viewport::new(10.0, if *key == 0 { 30.0 } else { 10.0 })
    });
    assert_ne!(tree.layout(first).rect.height, before);
    assert_ne!(tree.layout(second).rect, sibling_before);
    assert!(
        !tree.is_boundary(first),
        "a content change redistributes space between wrapped lines"
    );
}
