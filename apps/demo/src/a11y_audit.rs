//! Checks that every control a person can focus or click has an accessible name
//! (WCAG 4.1.2 Name, Role, Value), including widget parts exposed as their own nodes
//! through `Widget::accessibility_items`.

use std::collections::{HashMap, HashSet};

use noa_gpui_core::accesskit::{Action, Node, NodeId, Role};
use noa_gpui_core::widgets::Label;
use noa_gpui_core::{UiTree, WidgetId, access_id, access_item_id};

fn is_control(node: &Node) -> bool {
    node.supports_action(Action::Focus) || node.supports_action(Action::Click)
}

/// `Role::Label` nodes carry their text as the value.
fn is_named(node: &Node) -> bool {
    let filled = |s: Option<&str>| s.is_some_and(|s| !s.trim().is_empty());
    filled(node.label())
        || !node.labelled_by().is_empty()
        || (node.role() == Role::Label && filled(node.value()))
}

/// Results are in tree order. A widget part is reported as its widget with the
/// part's role.
pub fn unnamed_controls(tree: &UiTree) -> Vec<(WidgetId, Role)> {
    let update = tree.accessibility_update(1.0);
    let unnamed: HashMap<NodeId, Role> = update
        .nodes
        .iter()
        .filter(|(_, node)| is_control(node) && !is_named(node))
        .map(|(id, node)| (*id, node.role()))
        .collect();
    let nodes: HashSet<NodeId> = update.nodes.iter().map(|(id, _)| *id).collect();
    let mut found = Vec::new();
    let mut stack: Vec<WidgetId> = tree.root().into_iter().chain(tree.overlays()).collect();
    stack.reverse();
    while let Some(id) = stack.pop() {
        stack.extend(tree.children(id).iter().rev());
        if let Some(&role) = unnamed.get(&access_id(id)) {
            found.push((id, role));
        }
        let parts = (0..).map(|i| access_item_id(id, i));
        let parts = parts.take_while(|part| nodes.contains(part));
        found.extend(parts.filter_map(|part| Some((id, *unnamed.get(&part)?))));
    }
    found
}

/// The role of `id` and the text of the nearest label that is the last child of an
/// ancestor (a catalog card's caption), to locate it in the source.
pub fn describe(tree: &UiTree, (id, role): (WidgetId, Role)) -> String {
    let mut at = tree.parent(id);
    while let Some(ancestor) = at {
        let children = tree.children(ancestor);
        if let Some(label) = children.last().and_then(|&c| tree.get::<Label>(c))
            && children.len() > 1
        {
            return format!("{role:?} near \"{}\"", label.text());
        }
        at = tree.parent(ancestor);
    }
    format!("{role:?}")
}
