//! Mirrors the AccessKit tree as ARIA elements for the Web, where no AccessKit adapter
//! exists. [`AriaMirror`] keeps what the DOM holds and turns each [`TreeUpdate`] into
//! the few [`DomOp`]s that bring it up to date; it holds no DOM types, so it is tested
//! on the desktop. `web_a11y` (wasm32 only) carries the operations out on elements
//! inside the canvas, as its fallback content.

use std::collections::{BTreeMap, HashMap, HashSet};

use noa_gpui_core::accesskit::{
    Action, HasPopup, Live, Node, NodeId, Orientation, Role, Toggled, TreeUpdate,
};

/// The `id` attribute of the element mirroring `node`.
pub(crate) fn element_id(node: u64) -> String {
    format!("{ELEMENT_ID_PREFIX}{node}")
}

pub(crate) const ELEMENT_ID_PREFIX: &str = "noa-a11y-";

/// One change to the mirrored elements.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DomOp {
    /// Make an element for the node (its attributes follow as [`DomOp::Attr`]).
    Create(u64),
    /// Set (`Some`) or remove (`None`) an attribute.
    Attr {
        node: u64,
        name: &'static str,
        value: Option<String>,
    },
    /// Replace the element's own text.
    Text { node: u64, text: String },
    /// Append the element as the last child of `parent` (`None`: the container),
    /// moving it if it is elsewhere. Children are appended in order.
    Append { parent: Option<u64>, node: u64 },
    /// Remove the element; its node left the tree.
    Remove(u64),
    /// The node with keyboard focus (`None`: the root, no particular node).
    Focus(Option<u64>),
}

#[derive(Debug, Default)]
struct Mirrored {
    attrs: BTreeMap<&'static str, String>,
    text: String,
    children: Vec<NodeId>,
    labelled_by: Vec<NodeId>,
}

#[derive(Debug, Default)]
pub(crate) struct AriaMirror {
    nodes: HashMap<NodeId, Mirrored>,
    root: Option<NodeId>,
    focus: Option<NodeId>,
}

impl AriaMirror {
    /// Apply `update`, as given to an AccessKit adapter, and return what the DOM must do.
    pub fn update(&mut self, update: &TreeUpdate) -> Vec<DomOp> {
        let mut ops = Vec::new();
        let mut reordered = Vec::new();
        for (id, node) in &update.nodes {
            let (attrs, text) = describe(*id, node);
            let mirrored = self.nodes.entry(*id).or_insert_with(|| {
                ops.push(DomOp::Create(id.0));
                Mirrored::default()
            });
            for (name, value) in &attrs {
                if mirrored.attrs.get(name) != Some(value) {
                    ops.push(DomOp::Attr {
                        node: id.0,
                        name,
                        value: Some(value.clone()),
                    });
                }
            }
            for name in mirrored.attrs.keys() {
                if !attrs.contains_key(name) {
                    ops.push(DomOp::Attr {
                        node: id.0,
                        name,
                        value: None,
                    });
                }
            }
            if mirrored.text != text {
                ops.push(DomOp::Text {
                    node: id.0,
                    text: text.clone(),
                });
            }
            if mirrored.children != node.children() {
                reordered.push(*id);
            }
            *mirrored = Mirrored {
                attrs,
                text,
                children: node.children().to_vec(),
                labelled_by: node.labelled_by().to_vec(),
            };
        }
        if let Some(tree) = &update.tree
            && self.root != Some(tree.root)
        {
            self.root = Some(tree.root);
            ops.push(DomOp::Append {
                parent: None,
                node: tree.root.0,
            });
        }
        for parent in reordered {
            for child in &self.nodes[&parent].children {
                ops.push(DomOp::Append {
                    parent: Some(parent.0),
                    node: child.0,
                });
            }
        }
        self.remove_unreachable(&mut ops);
        let focus = Some(update.focus).filter(|focus| Some(*focus) != self.root);
        if focus != self.focus {
            self.focus = focus;
            ops.push(DomOp::Focus(focus.map(|f| f.0)));
        }
        ops
    }

    fn remove_unreachable(&mut self, ops: &mut Vec<DomOp>) {
        let mut reachable = HashSet::new();
        let mut stack: Vec<NodeId> = self.root.into_iter().collect();
        while let Some(id) = stack.pop() {
            if reachable.insert(id)
                && let Some(node) = self.nodes.get(&id)
            {
                stack.extend(&node.children);
            }
        }
        let mut gone: Vec<NodeId> = self
            .nodes
            .keys()
            .filter(|id| !reachable.contains(id))
            .copied()
            .collect();
        gone.sort();
        for id in gone {
            self.nodes.remove(&id);
            ops.push(DomOp::Remove(id.0));
        }
    }

    /// The accessible name of the focused node, for an element that takes DOM focus in
    /// its place (the IME text area), where `aria-activedescendant` cannot point.
    pub fn focused_name(&self) -> Option<String> {
        self.focus.and_then(|focus| self.name(focus))
    }

    /// The node's own label (or, for static text, its text), else the names of the
    /// nodes that label it.
    fn name(&self, node: NodeId) -> Option<String> {
        let mirrored = self.nodes.get(&node)?;
        if let Some(label) = mirrored.attrs.get("aria-label") {
            return Some(label.clone());
        }
        let by: Vec<String> = mirrored
            .labelled_by
            .iter()
            .filter_map(|by| {
                let by = self.nodes.get(by)?;
                by.attrs
                    .get("aria-label")
                    .cloned()
                    .or_else(|| (!by.text.is_empty()).then(|| by.text.clone()))
            })
            .collect();
        (!by.is_empty()).then(|| by.join(" "))
    }

    #[cfg(test)]
    fn attr(&self, node: NodeId, name: &str) -> Option<&str> {
        self.nodes.get(&node)?.attrs.get(name).map(String::as_str)
    }
}

/// The attributes and text of the element for `node`.
fn describe(id: NodeId, node: &Node) -> (BTreeMap<&'static str, String>, String) {
    let mut attrs = BTreeMap::new();
    let mut set = |name: &'static str, value: String| {
        attrs.insert(name, value);
    };
    let role = node.role();
    set("id", element_id(id.0));
    if let Some(aria) = aria_role(role) {
        set("role", aria.to_owned());
    }
    let label = node.label().filter(|l| !l.is_empty());
    let value = node.value().filter(|v| !v.is_empty());
    // Static text and text inputs are read from their content; other elements from
    // their name and value.
    let text = match role {
        Role::Label => label.or(value).unwrap_or_default().to_owned(),
        _ if is_text_input(role) || role == Role::ComboBox => {
            if let Some(label) = label {
                set("aria-label", label.to_owned());
            }
            value.unwrap_or_default().to_owned()
        }
        _ => {
            if let Some(label) = label {
                set("aria-label", label.to_owned());
            }
            if let Some(value) = value {
                set("aria-valuetext", value.to_owned());
            }
            String::new()
        }
    };
    if !node.labelled_by().is_empty() {
        let ids: Vec<String> = node
            .labelled_by()
            .iter()
            .map(|by| element_id(by.0))
            .collect();
        set("aria-labelledby", ids.join(" "));
    }
    if role == Role::MultilineTextInput {
        set("aria-multiline", "true".into());
    }
    if let Some(description) = node.description() {
        set("aria-description", description.to_owned());
    }
    if let Some(placeholder) = node.placeholder() {
        set("aria-placeholder", placeholder.to_owned());
    }
    if let Some(toggled) = node.toggled() {
        let state = match toggled {
            Toggled::True => "true",
            Toggled::False => "false",
            Toggled::Mixed => "mixed",
        };
        let name = if matches!(role, Role::Button | Role::DisclosureTriangle) {
            "aria-pressed"
        } else {
            "aria-checked"
        };
        set(name, state.into());
    }
    let flag = |value: bool| value.to_string();
    if let Some(expanded) = node.is_expanded() {
        set("aria-expanded", flag(expanded));
    }
    if let Some(selected) = node.is_selected() {
        set("aria-selected", flag(selected));
    }
    for (on, name) in [
        (node.is_disabled(), "aria-disabled"),
        (node.is_required(), "aria-required"),
        (node.is_read_only(), "aria-readonly"),
        (node.is_modal(), "aria-modal"),
        (node.is_multiselectable(), "aria-multiselectable"),
        (node.is_hidden(), "aria-hidden"),
    ] {
        if on {
            set(name, "true".into());
        }
    }
    if let Some(popup) = node.has_popup() {
        let popup = match popup {
            HasPopup::Menu => "menu",
            HasPopup::Listbox => "listbox",
            HasPopup::Tree => "tree",
            HasPopup::Grid => "grid",
            HasPopup::Dialog => "dialog",
        };
        set("aria-haspopup", popup.into());
    }
    for (number, name) in [
        (node.numeric_value(), "aria-valuenow"),
        (node.min_numeric_value(), "aria-valuemin"),
        (node.max_numeric_value(), "aria-valuemax"),
    ] {
        if let Some(number) = number {
            set(name, number.to_string());
        }
    }
    if let Some(level) = node.level() {
        set("aria-level", level.to_string());
    }
    // ARIA counts rows from 1.
    if let Some(row) = node.row_index() {
        set("aria-rowindex", (row + 1).to_string());
    }
    if let Some(rows) = node.row_count() {
        set("aria-rowcount", rows.to_string());
    }
    if let Some(orientation) = node.orientation() {
        let orientation = match orientation {
            Orientation::Horizontal => "horizontal",
            Orientation::Vertical => "vertical",
        };
        set("aria-orientation", orientation.into());
    }
    if let Some(live) = node.live() {
        let live = match live {
            Live::Off => "off",
            Live::Polite => "polite",
            Live::Assertive => "assertive",
        };
        set("aria-live", live.into());
    }
    // Focusable from assistive technology; never in the page's own Tab order, which
    // the canvas keeps.
    if node.supports_action(Action::Focus) || node.supports_action(Action::Click) {
        set("tabindex", "-1".into());
    }
    (attrs, text)
}

fn is_text_input(role: Role) -> bool {
    matches!(
        role,
        Role::TextInput
            | Role::SearchInput
            | Role::PasswordInput
            | Role::MultilineTextInput
            | Role::DateInput
            | Role::TimeInput
            | Role::DateTimeInput
    )
}

/// The ARIA role for an AccessKit role; `None` leaves a plain `div`.
fn aria_role(role: Role) -> Option<&'static str> {
    Some(match role {
        Role::Button | Role::ColorWell | Role::DisclosureTriangle => "button",
        Role::CheckBox => "checkbox",
        Role::Switch => "switch",
        Role::RadioButton => "radio",
        Role::RadioGroup => "radiogroup",
        Role::SearchInput => "searchbox",
        Role::TextInput
        | Role::PasswordInput
        | Role::MultilineTextInput
        | Role::DateInput
        | Role::TimeInput
        | Role::DateTimeInput => "textbox",
        Role::ComboBox => "combobox",
        Role::Link => "link",
        Role::Image => "img",
        Role::Figure => "figure",
        Role::Slider => "slider",
        Role::SpinButton => "spinbutton",
        Role::ProgressIndicator => "progressbar",
        Role::Meter => "meter",
        Role::Dialog => "dialog",
        Role::AlertDialog => "alertdialog",
        Role::Alert => "alert",
        Role::Status => "status",
        Role::Log => "log",
        Role::Tooltip => "tooltip",
        Role::Toolbar => "toolbar",
        Role::Navigation => "navigation",
        Role::Menu => "menu",
        Role::MenuBar => "menubar",
        Role::MenuItem => "menuitem",
        Role::MenuItemCheckBox => "menuitemcheckbox",
        Role::MenuItemRadio => "menuitemradio",
        Role::List => "list",
        Role::ListItem => "listitem",
        Role::ListBox => "listbox",
        Role::ListBoxOption => "option",
        Role::Tree => "tree",
        Role::TreeItem => "treeitem",
        Role::Table => "table",
        Role::Grid => "grid",
        Role::Row => "row",
        Role::Cell => "cell",
        Role::GridCell => "gridcell",
        Role::ColumnHeader => "columnheader",
        Role::RowHeader => "rowheader",
        Role::TabList => "tablist",
        Role::Tab => "tab",
        Role::TabPanel => "tabpanel",
        Role::Group => "group",
        Role::Splitter => "separator",
        Role::Heading => "heading",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use noa_gpui_core::widgets::{
        Button, CheckState, Checkbox, Container, FormField, Label, TextField,
    };
    use noa_gpui_core::{ACCESS_ROOT, UiTree, WidgetId, access_id};
    use noa_gpui_scene::Size;
    use noa_gpui_text::TextStore;

    struct Page {
        tree: UiTree,
        text: TextStore,
        root: WidgetId,
        mirror: AriaMirror,
    }

    impl Page {
        fn new() -> Self {
            let mut tree = UiTree::new();
            let root = tree.insert(None, Container::new(Default::default()));
            Self {
                tree,
                text: TextStore::new(Default::default()),
                root,
                mirror: AriaMirror::default(),
            }
        }

        fn sync(&mut self) -> Vec<DomOp> {
            self.tree.prepare(Size::new(400.0, 300.0), &mut self.text);
            self.mirror.update(&self.tree.accessibility_update(1.0))
        }
    }

    fn attr(node: WidgetId, name: &'static str, value: &str) -> DomOp {
        DomOp::Attr {
            node: access_id(node).0,
            name,
            value: Some(value.into()),
        }
    }

    #[test]
    fn the_first_update_builds_every_element_with_its_role_and_name() {
        let mut page = Page::new();
        let button = page.tree.insert(Some(page.root), Button::new("Save"));
        let label = page.tree.insert(Some(page.root), Label::new("Ready"));
        let check = page
            .tree
            .insert(Some(page.root), Checkbox::new("Wrap lines").checked(true));
        let ops = page.sync();

        assert!(ops.contains(&DomOp::Create(ACCESS_ROOT.0)));
        for id in [button, label, check] {
            assert!(ops.contains(&DomOp::Create(access_id(id).0)), "{id:?}");
        }
        assert!(ops.contains(&attr(button, "role", "button")));
        assert!(ops.contains(&attr(button, "aria-label", "Save")));
        assert!(ops.contains(&attr(button, "tabindex", "-1")));
        assert!(ops.contains(&DomOp::Text {
            node: access_id(label).0,
            text: "Ready".into()
        }));
        assert!(ops.contains(&attr(check, "role", "checkbox")));
        assert!(ops.contains(&attr(check, "aria-checked", "true")));

        // The window goes into the container and the widgets into it in order (the
        // role-less container between them is left out), all after they exist.
        let position = |op: &DomOp| ops.iter().position(|o| o == op).unwrap();
        let root = ACCESS_ROOT.0;
        let appended: Vec<u64> = ops
            .iter()
            .filter_map(|op| match op {
                DomOp::Append {
                    parent: Some(parent),
                    node,
                } if *parent == root => Some(*node),
                _ => None,
            })
            .collect();
        assert_eq!(
            appended,
            [button, label, check].map(|id| access_id(id).0).to_vec()
        );
        assert!(
            position(&DomOp::Create(access_id(check).0))
                < position(&DomOp::Append {
                    parent: None,
                    node: root
                })
        );
    }

    #[test]
    fn later_updates_change_only_what_changed() {
        let mut page = Page::new();
        let label = page.tree.insert(Some(page.root), Label::new("Ready"));
        let check = page
            .tree
            .insert(Some(page.root), Checkbox::new("Wrap lines"));
        page.sync();

        page.tree
            .update::<Label, _>(label, |l, cx| l.set_text(cx, "Saved"));
        page.tree
            .update::<Checkbox, _>(check, |c, cx| c.set_state(cx, CheckState::On));
        let ops = page.sync();
        let changes: Vec<&DomOp> = ops
            .iter()
            .filter(|op| !matches!(op, DomOp::Focus(_)))
            .collect();
        assert_eq!(changes.len(), 2, "{changes:?}");
        assert!(ops.contains(&DomOp::Text {
            node: access_id(label).0,
            text: "Saved".into()
        }));
        assert!(ops.contains(&attr(check, "aria-checked", "true")));
        assert!(!ops.iter().any(|op| matches!(op, DomOp::Create(_))));
        assert!(page.sync().is_empty(), "nothing changed");
    }

    #[test]
    fn removed_widgets_remove_their_elements_and_focus_follows() {
        let mut page = Page::new();
        let first = page.tree.insert(Some(page.root), Button::new("One"));
        let second = page.tree.insert(Some(page.root), Button::new("Two"));
        page.sync();

        page.tree.set_focus(Some(second));
        let ops = page.sync();
        assert!(ops.contains(&DomOp::Focus(Some(access_id(second).0))));

        page.tree.remove(second, &mut page.text);
        let ops = page.sync();
        assert!(ops.contains(&DomOp::Remove(access_id(second).0)));
        assert!(ops.contains(&DomOp::Focus(None)));
        assert!(!ops.contains(&DomOp::Remove(access_id(first).0)));
        assert_eq!(
            page.mirror.attr(access_id(first), "aria-label"),
            Some("One")
        );
        assert_eq!(page.mirror.attr(access_id(second), "aria-label"), None);
    }

    #[test]
    fn controls_named_by_a_form_field_point_at_it_and_lend_its_name_to_focus() {
        let mut page = Page::new();
        let form = page.tree.insert(Some(page.root), FormField::new("Email"));
        let field = page.tree.insert(Some(form), TextField::new("", 200.0));
        let ops = page.sync();
        assert!(ops.contains(&DomOp::Attr {
            node: access_id(field).0,
            name: "aria-labelledby",
            value: Some(element_id(access_id(form).0)),
        }));
        assert_eq!(page.mirror.focused_name(), None);

        page.tree.set_focus(Some(field));
        page.sync();
        assert_eq!(page.mirror.focused_name().as_deref(), Some("Email"));
    }
}
