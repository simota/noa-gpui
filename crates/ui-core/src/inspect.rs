//! Looking inside a running widget tree: what a widget is, where it is, which style
//! rules shape it, and the tree as an outline. The platform's inspector (see
//! `noa_gpui_platform::App::inspector`) shows this for the widget under the pointer.

use std::fmt::Write as _;

use noa_gpui_scene::Rect;

use crate::styling::{MatchedRule, StyleState, StyleTree as _};
use crate::tree::{UiTree, WidgetId};

/// What [`UiTree::inspect`] found out about one widget.
#[derive(Debug, Clone, PartialEq)]
pub struct Inspection {
    pub id: WidgetId,
    /// The type selectors match (`Button`, `TextField`, ...).
    pub widget_type: String,
    /// The `#id` selectors match, if one was given.
    pub style_id: Option<String>,
    /// The `.class` selectors match.
    pub classes: Vec<String>,
    /// Where it is in the window, in logical points.
    pub rect: Rect,
    /// The pseudo-classes in effect (`:hover`, `:focus`, `:disabled`, ...).
    pub state: StyleState,
    pub hidden: bool,
    /// The style rules that apply, lowest precedence first.
    pub rules: Vec<MatchedRule>,
    /// Declarations set on the widget itself (`UiTree::set_inline_style`).
    pub has_inline_style: bool,
    /// Its ancestors' types, nearest first.
    pub ancestors: Vec<String>,
    pub children: usize,
}

impl Inspection {
    /// The widget as a selector would name it: `Button#save.primary`.
    pub fn selector(&self) -> String {
        let mut out = self.widget_type.clone();
        if let Some(id) = &self.style_id {
            let _ = write!(out, "#{id}");
        }
        for class in &self.classes {
            let _ = write!(out, ".{class}");
        }
        out
    }

    /// The pseudo-classes in effect, as written in a selector (`:hover:focus`).
    pub fn pseudo_classes(&self) -> String {
        let s = self.state;
        [
            (s.hovered, ":hover"),
            (s.focused, ":focus"),
            (s.focus_within && !s.focused, ":focus-within"),
            (s.focus_visible, ":focus-visible"),
            (s.active, ":active"),
            (s.disabled, ":disabled"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }

    /// Several lines for a panel or a log: the selector and state, the frame, the path
    /// from the root, and the rules that apply (strongest last).
    pub fn describe(&self) -> String {
        let mut out = format!("{}{}", self.selector(), self.pseudo_classes());
        if self.hidden {
            out.push_str(" (hidden)");
        }
        let r = self.rect;
        let _ = write!(
            out,
            "\n{} × {} at ({}, {}) · {} children",
            r.width, r.height, r.x, r.y, self.children
        );
        if !self.ancestors.is_empty() {
            let path: Vec<&str> = self.ancestors.iter().rev().map(String::as_str).collect();
            let _ = write!(out, "\nin {}", path.join(" › "));
        }
        if self.rules.is_empty() && !self.has_inline_style {
            out.push_str("\nno style rules");
        }
        for rule in &self.rules {
            let media = if rule.in_media { " (@media)" } else { "" };
            let _ = write!(
                out,
                "\n{}{media} {{ {} }}",
                rule.selector,
                rule.declarations.join("; ")
            );
        }
        if self.has_inline_style {
            out.push_str("\ninline style");
        }
        out
    }
}

impl UiTree {
    /// What `id` is, where it is and which style rules apply to it; `None` for a
    /// removed widget.
    pub fn inspect(&self, id: WidgetId) -> Option<Inspection> {
        let rect = self.window_rect(id)?;
        let node = self.style_node(id);
        let (style_id, classes) = (
            node.meta.id_value().map(str::to_owned),
            node.meta.classes().to_vec(),
        );
        Some(Inspection {
            id,
            widget_type: node.widget_type.to_owned(),
            style_id,
            classes,
            rect,
            state: node.state,
            hidden: self.is_hidden(id),
            rules: self.stylesheet().matching_rules(self, id),
            has_inline_style: self.has_inline_style(id),
            ancestors: self
                .ancestors(id)
                .skip(1)
                .map(|a| self.style_node(a).widget_type.to_owned())
                .collect(),
            children: self.children(id).len(),
        })
    }

    /// The tree from the root as an indented outline, one widget per line with its
    /// selector and frame, overlays last. For logs and bug reports.
    pub fn outline(&self) -> String {
        let mut out = String::new();
        let roots = self.root().into_iter().chain(self.overlays());
        for root in roots {
            let mut stack = vec![(root, 0usize)];
            while let Some((id, depth)) = stack.pop() {
                let Some(info) = self.inspect(id) else {
                    continue;
                };
                let r = info.rect;
                let _ = writeln!(
                    out,
                    "{}{} {}×{} at ({}, {}){}",
                    "  ".repeat(depth),
                    info.selector(),
                    r.width,
                    r.height,
                    r.x,
                    r.y,
                    if info.hidden { " hidden" } else { "" }
                );
                stack.extend(self.children(id).iter().rev().map(|c| (*c, depth + 1)));
            }
        }
        out
    }
}
