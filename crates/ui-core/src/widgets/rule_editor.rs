use std::borrow::Cow;

use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Size as LayoutSize, length};
use noa_gpui_scene::{Border, Direction, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::editable::SingleLine;
use super::table::leading_number;
use super::{CachedText, MenuItem, pressed_tint};
use crate::event::{Event, Key, PointerButton};
use crate::overlay::Placement;
use crate::theme::Role;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

pub const RULE_ROW: f32 = 36.0;
const CONTROL: f32 = 26.0;
const FIELD_WIDTH: f32 = 130.0;
const OPERATOR_WIDTH: f32 = 120.0;
const BUTTON: f32 = 20.0;
const GAP: f32 = 8.0;
const INSET: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    Text,
    Number,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleField {
    pub name: String,
    pub kind: RuleKind,
}

impl RuleField {
    pub fn text(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: RuleKind::Text,
        }
    }

    pub fn number(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: RuleKind::Number,
        }
    }
}

/// Text comparisons ignore case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOperator {
    Contains,
    BeginsWith,
    EndsWith,
    Is,
    IsNot,
    Equal,
    NotEqual,
    Less,
    Greater,
}

impl RuleOperator {
    const TEXT: [RuleOperator; 5] = [
        RuleOperator::Contains,
        RuleOperator::BeginsWith,
        RuleOperator::EndsWith,
        RuleOperator::Is,
        RuleOperator::IsNot,
    ];
    const NUMBER: [RuleOperator; 4] = [
        RuleOperator::Equal,
        RuleOperator::NotEqual,
        RuleOperator::Less,
        RuleOperator::Greater,
    ];

    pub fn for_kind(kind: RuleKind) -> &'static [RuleOperator] {
        match kind {
            RuleKind::Text => &Self::TEXT,
            RuleKind::Number => &Self::NUMBER,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RuleOperator::Contains => "contains",
            RuleOperator::BeginsWith => "begins with",
            RuleOperator::EndsWith => "ends with",
            RuleOperator::Is => "is",
            RuleOperator::IsNot => "is not",
            RuleOperator::Equal => "is equal to",
            RuleOperator::NotEqual => "is not equal to",
            RuleOperator::Less => "is less than",
            RuleOperator::Greater => "is greater than",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuleMatch {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub field: usize,
    pub operator: RuleOperator,
    pub value: String,
}

/// A copy of a [`RuleEditor`]'s rules ([`RuleEditor::matcher`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuleMatcher {
    pub rules: Vec<Rule>,
    pub matching: RuleMatch,
}

impl RuleMatcher {
    /// Rules are asked for in order and stop at the first that decides.
    pub fn matches<'a>(&self, cell: impl Fn(usize) -> Cow<'a, str>) -> bool {
        let mut results = self.rules.iter().map(|rule| rule.test(&cell(rule.field)));
        match self.matching {
            RuleMatch::All => results.all(|ok| ok),
            RuleMatch::Any => results.any(|ok| ok),
        }
    }
}

impl Rule {
    /// Whether `cell` (the field's value) passes. An empty value passes everything,
    /// so a rule being typed does not hide all rows. Number rules compare the numbers
    /// the cell and the value start with ("12 KB" is 12), and fail when either has none.
    pub fn test(&self, cell: &str) -> bool {
        if self.value.trim().is_empty() {
            return true;
        }
        let number = leading_number;
        let (a, b) = (cell.to_lowercase(), self.value.trim().to_lowercase());
        match self.operator {
            RuleOperator::Contains => a.contains(&b),
            RuleOperator::BeginsWith => a.starts_with(&b),
            RuleOperator::EndsWith => a.ends_with(&b),
            RuleOperator::Is => a == b,
            RuleOperator::IsNot => a != b,
            op => match (number(cell), number(&self.value)) {
                (Some(x), Some(y)) => match op {
                    RuleOperator::Equal => x == y,
                    RuleOperator::NotEqual => x != y,
                    RuleOperator::Less => x < y,
                    _ => x > y,
                },
                _ => false,
            },
        }
    }
}

/// Emitted when the rules or the All / Any choice change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulesChanged;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Popup {
    Match,
    Field(usize),
    Operator(usize),
}

#[derive(Debug)]
struct RuleRow {
    field: usize,
    operator: RuleOperator,
    value: SingleLine,
}

/// Search criteria as rows (`NSPredicateEditor`). Emits [`RulesChanged`].
#[derive(Debug)]
pub struct RuleEditor {
    fields: Vec<RuleField>,
    rows: Vec<RuleRow>,
    /// Updates have no TextStore; release old layouts on the next paint/removal.
    retired: Vec<noa_gpui_scene::TextLayoutId>,
    matching: RuleMatch,
    width: f32,
    active: usize,
    menu: Option<Popup>,
    labels: Vec<CachedText>,
}

/// The label slots: the match row's three texts, then two popup titles per rule row.
const MATCH_LABELS: usize = 3;

impl RuleEditor {
    /// Start with an empty rule for the first field. With no fields, show no rule
    /// rows and accept no text input.
    pub fn new(fields: Vec<RuleField>, width: f32) -> Self {
        let mut editor = Self {
            fields,
            rows: Vec::new(),
            retired: Vec::new(),
            matching: RuleMatch::All,
            width,
            active: 0,
            menu: None,
            labels: Vec::new(),
        };
        if !editor.fields.is_empty() {
            editor.rows.push(editor.new_row(0, None, ""));
        }
        editor
    }

    /// Replace the rules with these (builder). Rules for unknown fields are ignored;
    /// if no valid rules remain, keep the current rules.
    pub fn rules_from(mut self, rules: impl IntoIterator<Item = Rule>) -> Self {
        let rows: Vec<RuleRow> = rules
            .into_iter()
            .filter(|r| r.field < self.fields.len())
            .map(|r| self.new_row(r.field, Some(r.operator), &r.value))
            .collect();
        if !rows.is_empty() {
            self.replace_rows(rows);
        }
        self
    }

    pub fn matching(mut self, matching: RuleMatch) -> Self {
        self.matching = matching;
        self
    }

    pub fn fields(&self) -> &[RuleField] {
        &self.fields
    }

    pub fn match_kind(&self) -> RuleMatch {
        self.matching
    }

    pub fn rules(&self) -> Vec<Rule> {
        self.rows
            .iter()
            .map(|r| Rule {
                field: r.field,
                operator: r.operator,
                value: r.value.edit.editor.text().to_owned(),
            })
            .collect()
    }

    pub fn matches<'a>(&self, cell: impl Fn(usize) -> Cow<'a, str>) -> bool {
        self.matcher().matches(cell)
    }

    /// The rules as they are now, apart from the editor: to keep matching records
    /// with them after it changes, such as in [`super::TableView::set_filter`].
    pub fn matcher(&self) -> RuleMatcher {
        RuleMatcher {
            rules: self.rules(),
            matching: self.matching,
        }
    }

    /// Replace the rules, ignoring unknown fields. If no valid rules remain, keep
    /// the current rules, as for an empty input. Choices from an old popup are ignored.
    pub fn set_rules(&mut self, cx: &mut UpdateCx, rules: Vec<Rule>) {
        let rows: Vec<RuleRow> = rules
            .into_iter()
            .filter(|r| r.field < self.fields.len())
            .map(|r| self.new_row(r.field, Some(r.operator), &r.value))
            .collect();
        if !rows.is_empty() {
            self.replace_rows(rows);
            cx.request_layout();
            cx.request_paint();
        }
    }

    fn replace_rows(&mut self, rows: Vec<RuleRow>) {
        for row in std::mem::replace(&mut self.rows, rows) {
            if let Some(layout) = row.value.edit.shaped.layout() {
                self.retired.push(layout);
            }
        }
        self.active = 0;
        self.menu = None;
    }

    pub fn active_row(&self) -> usize {
        self.active
    }

    fn new_row(&self, field: usize, operator: Option<RuleOperator>, value: &str) -> RuleRow {
        let kind = self.fields.get(field).map_or(RuleKind::Text, |f| f.kind);
        let mut line = SingleLine::new(String::new(), 100.0, 6.0);
        line.edit.style = TextStyle::new(13.0);
        line.edit.editor.set_text(value);
        RuleRow {
            field,
            operator: operator.unwrap_or(RuleOperator::for_kind(kind)[0]),
            value: line,
        }
    }

    fn height(&self) -> f32 {
        (self.rows.len() + 1) as f32 * RULE_ROW + 8.0
    }

    /// In widget coordinates: field, operator, value, remove and add.
    fn row_rects(&self, row: usize) -> [Rect; 5] {
        let y = (row + 1) as f32 * RULE_ROW + 4.0;
        let cy = y + (RULE_ROW - CONTROL) / 2.0;
        let x0 = INSET + 12.0;
        let field = Rect::new(x0, cy, FIELD_WIDTH, CONTROL);
        let op = Rect::new(field.right() + GAP, cy, OPERATOR_WIDTH, CONTROL);
        let add = Rect::new(
            self.width - INSET - BUTTON,
            y + (RULE_ROW - BUTTON) / 2.0,
            BUTTON,
            BUTTON,
        );
        let remove = Rect::new(add.x - GAP - BUTTON, add.y, BUTTON, BUTTON);
        let value = Rect::new(
            op.right() + GAP,
            cy,
            (remove.x - GAP - op.right() - GAP).max(40.0),
            CONTROL,
        );
        [field, op, value, remove, add]
    }

    fn match_rect(&self) -> Rect {
        Rect::new(
            INSET + 48.0,
            4.0 + (RULE_ROW - CONTROL) / 2.0,
            64.0,
            CONTROL,
        )
    }

    fn popup_rect(&self, popup: Popup) -> Rect {
        match popup {
            Popup::Match => self.match_rect(),
            Popup::Field(r) => self.row_rects(r)[0],
            Popup::Operator(r) => self.row_rects(r)[1],
        }
    }

    /// In widget coordinates.
    fn value_origin(&self, row: usize) -> Point {
        let rect = self.row_rects(row)[2];
        let line = &self.rows[row].value;
        Point::new(
            rect.x + line.pad_left - line.scroll_x,
            rect.y + (CONTROL - line.edit.style.line_height) / 2.0,
        )
    }

    fn open(&mut self, cx: &mut EventCx, popup: Popup) {
        let (items, checked): (Vec<MenuItem>, usize) = match popup {
            Popup::Match => (
                vec![MenuItem::new("All"), MenuItem::new("Any")],
                usize::from(self.matching == RuleMatch::Any),
            ),
            Popup::Field(r) => (
                self.fields.iter().map(|f| MenuItem::new(&f.name)).collect(),
                self.rows[r].field,
            ),
            Popup::Operator(r) => {
                let ops = RuleOperator::for_kind(self.fields[self.rows[r].field].kind);
                (
                    ops.iter().map(|o| MenuItem::new(o.label())).collect(),
                    ops.iter()
                        .position(|&o| o == self.rows[r].operator)
                        .unwrap_or(0),
                )
            }
        };
        let rect = self.popup_rect(popup);
        let origin = cx.window_rect();
        let anchor = Rect::new(
            origin.x + rect.x,
            origin.y + rect.y,
            rect.width,
            rect.height,
        );
        self.menu = Some(popup);
        cx.open_menu(items, Placement::Below(anchor), rect.width, Some(checked));
        cx.request_paint();
    }

    fn chosen(&mut self, cx: &mut EventCx, popup: Popup, index: usize) {
        match popup {
            Popup::Match => {
                self.matching = if index == 0 {
                    RuleMatch::All
                } else {
                    RuleMatch::Any
                };
            }
            Popup::Field(r) => {
                if self.rows[r].field == index {
                    return;
                }
                let kind = self.fields[index].kind;
                let row = &mut self.rows[r];
                if self.fields[row.field].kind != kind {
                    row.operator = RuleOperator::for_kind(kind)[0];
                }
                row.field = index;
            }
            Popup::Operator(r) => {
                let ops = RuleOperator::for_kind(self.fields[self.rows[r].field].kind);
                self.rows[r].operator = ops[index];
            }
        }
        cx.emit(RulesChanged);
        cx.request_paint();
    }

    fn add_after(&mut self, cx: &mut EventCx, row: usize) {
        let field = self.rows[row].field;
        let new = self.new_row(field, None, "");
        self.forward(cx, &Event::FocusChanged(false));
        self.rows.insert(row + 1, new);
        self.active = row + 1;
        cx.emit(RulesChanged);
        cx.request_layout();
        cx.request_paint();
    }

    fn remove(&mut self, cx: &mut EventCx, row: usize) {
        if self.rows.len() <= 1 {
            return;
        }
        if self.active == row {
            self.forward(cx, &Event::FocusChanged(false));
        }
        let mut gone = self.rows.remove(row);
        gone.value.edit.removed(cx.text_store());
        if self.active > row {
            self.active -= 1;
        }
        self.active = self.active.min(self.rows.len() - 1);
        cx.emit(RulesChanged);
        cx.request_layout();
        cx.request_paint();
    }

    fn activate(&mut self, cx: &mut EventCx, row: usize) {
        if self.active != row {
            self.forward(cx, &Event::FocusChanged(false));
            self.active = row;
            cx.request_paint();
        }
    }

    fn forward(&mut self, cx: &mut EventCx, event: &Event) {
        let row = self.active;
        let Some(active) = self.rows.get(row) else {
            return;
        };
        let before = active.value.edit.editor.text().to_owned();
        let origin = self.value_origin(row);
        self.rows[row].value.edit.event(cx, event, origin);
        if self.rows[row].value.edit.editor.text() != before {
            cx.emit(RulesChanged);
        }
        cx.request_paint();
    }

    fn label(&mut self, cx: &mut PaintCx, slot: usize, text: &str, style: TextStyle) -> Size {
        while self.labels.len() <= slot {
            self.labels.push(CachedText::default());
        }
        self.labels[slot].shape(cx.text_store(), text, style, None)
    }

    fn draw_label(&mut self, cx: &mut PaintCx, slot: usize, at: Point, height: f32, role: Role) {
        if let Some(layout) = self.labels[slot].layout() {
            let size = self.labels[slot].size();
            let ink = cx.color(role);
            cx.text(
                Rect::new(
                    at.x,
                    at.y + (height - size.height) / 2.0,
                    size.width,
                    size.height,
                ),
                layout,
                ink,
            );
        }
    }

    fn paint_popup(&mut self, cx: &mut PaintCx, popup: Popup, slot: usize, title: &str) {
        let rect = self.popup_rect(popup);
        let surface = cx.color(Role::Surface);
        let fill = if self.menu == Some(popup) {
            pressed_tint(cx, surface)
        } else {
            surface
        };
        let outline = cx.color(Role::Outline);
        cx.rounded_rect(
            rect,
            6.0,
            fill,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        self.label(cx, slot, title, TextStyle::new(13.0));
        cx.push_clip(Rect::new(
            rect.x + 8.0,
            rect.y,
            rect.width - 28.0,
            rect.height,
        ));
        self.draw_label(
            cx,
            slot,
            Point::new(rect.x + 9.0, rect.y),
            rect.height,
            Role::Text,
        );
        cx.pop_clip();
        let muted = cx.color(Role::Muted);
        let ax = rect.right() - 16.0;
        let cy = rect.y + rect.height / 2.0;
        cx.chevron(Rect::new(ax, cy - 6.0, 7.0, 3.5), Direction::Up, 1.4, muted);
        cx.chevron(
            Rect::new(ax, cy + 2.5, 7.0, 3.5),
            Direction::Down,
            1.4,
            muted,
        );
    }

    fn paint_button(&self, cx: &mut PaintCx, rect: Rect, plus: bool, enabled: bool) {
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        cx.rounded_rect(
            rect,
            rect.width / 2.0,
            surface,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let ink = cx.color(Role::Text);
        let ink = if enabled { ink } else { ink.with_alpha(0.3) };
        let (cx0, cy0) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        cx.line(
            Point::new(cx0 - 5.0, cy0),
            Point::new(cx0 + 5.0, cy0),
            1.5,
            ink,
        );
        if plus {
            cx.line(
                Point::new(cx0, cy0 - 5.0),
                Point::new(cx0, cy0 + 5.0),
                1.5,
                ink,
            );
        }
    }
}

impl Widget for RuleEditor {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: length(self.width),
                height: length(self.height()),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        for layout in self.retired.drain(..) {
            cx.text_store().remove(layout);
        }
        let bounds = cx.bounds();
        let well = cx.color(Role::SurfaceAlt);
        let line = cx.color(Role::Separator);
        cx.rounded_rect(
            bounds,
            8.0,
            well,
            Border {
                width: 1.0,
                color: line,
            },
        );
        let style = TextStyle::new(13.0);
        let top = 4.0;
        self.label(cx, 0, "Match", style);
        self.draw_label(cx, 0, Point::new(INSET, top), RULE_ROW, Role::Text);
        let matching = if self.matching == RuleMatch::All {
            "All"
        } else {
            "Any"
        };
        self.paint_popup(cx, Popup::Match, 1, matching);
        self.label(cx, 2, "of the following rules:", style);
        let after = self.match_rect().right() + GAP;
        self.draw_label(cx, 2, Point::new(after, top), RULE_ROW, Role::Text);

        let focused = cx.is_focused();
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        let accent = cx.color(Role::Accent);
        for r in 0..self.rows.len() {
            let y = (r + 1) as f32 * RULE_ROW + 4.0;
            cx.fill_rect(Rect::new(INSET, y, self.width - 2.0 * INSET, 1.0), line);
            let field = self.fields[self.rows[r].field].name.clone();
            let op = self.rows[r].operator.label();
            let slot = MATCH_LABELS + r * 2;
            self.paint_popup(cx, Popup::Field(r), slot, &field);
            self.paint_popup(cx, Popup::Operator(r), slot + 1, op);
            let [_, _, value, remove, add] = self.row_rects(r);
            let typing = focused && r == self.active;
            cx.rounded_rect(
                value,
                5.0,
                surface,
                if typing {
                    Border {
                        width: 2.0,
                        color: accent,
                    }
                } else {
                    Border {
                        width: 1.0,
                        color: outline,
                    }
                },
            );
            let row = &mut self.rows[r].value;
            row.width = value.width;
            row.measure(cx.text_store());
            let origin = self.value_origin(r);
            cx.push_clip(Rect::new(
                value.x + 4.0,
                value.y,
                value.width - 8.0,
                value.height,
            ));
            self.rows[r].value.edit.paint(cx, origin, typing);
            cx.pop_clip();
            self.paint_button(cx, remove, false, self.rows.len() > 1);
            self.paint_button(cx, add, true, true);
        }
        let used = MATCH_LABELS + self.rows.len() * 2;
        if self.labels.len() > used {
            for mut label in self.labels.drain(used..) {
                label.release(cx.text_store());
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::FocusChanged(false) => self.forward(cx, event),
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                if self.match_rect().contains(p) {
                    self.open(cx, Popup::Match);
                    cx.set_handled();
                    return;
                }
                for r in 0..self.rows.len() {
                    let [field, op, value, remove, add] = self.row_rects(r);
                    if field.contains(p) {
                        self.open(cx, Popup::Field(r));
                    } else if op.contains(p) {
                        self.open(cx, Popup::Operator(r));
                    } else if remove.contains(p) {
                        self.remove(cx, r);
                    } else if add.contains(p) {
                        self.add_after(cx, r);
                    } else if value.contains(p) {
                        self.activate(cx, r);
                        self.forward(cx, event);
                    } else {
                        continue;
                    }
                    cx.set_handled();
                    return;
                }
            }
            Event::PointerMove { .. } | Event::PointerUp { .. } if cx.has_capture() => {
                self.forward(cx, event);
            }
            Event::MenuClosed { chosen } => {
                if let Some(popup) = self.menu.take() {
                    cx.request_paint();
                    if let Some(index) = chosen {
                        self.chosen(cx, popup, *index);
                    }
                }
            }
            Event::KeyDown {
                key: Key::Tab,
                modifiers,
            } if cx.is_focused() => {
                if self
                    .rows
                    .get(self.active)
                    .is_some_and(|row| !row.value.edit.composition().is_empty())
                {
                    self.forward(cx, event);
                    return;
                }
                let next = if modifiers.shift {
                    self.active.checked_sub(1)
                } else {
                    Some(self.active + 1).filter(|&n| n < self.rows.len())
                };
                if let Some(next) = next {
                    self.activate(cx, next);
                    cx.set_handled();
                }
            }
            Event::KeyDown { .. }
            | Event::Text(_)
            | Event::Composition { .. }
            | Event::ImeSelect(_)
                if cx.is_focused() =>
            {
                self.forward(cx, event);
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        self.rows.get(self.active)?;
        let origin = self.value_origin(self.active);
        Some(self.rows[self.active].value.edit.input_area(origin))
    }

    fn ime_context(&self, text: &TextStore) -> Option<crate::ImeContext> {
        self.rows
            .get(self.active)?
            .value
            .edit
            .ime_context(text, self.value_origin(self.active))
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let rules: Vec<String> = self
            .rules()
            .iter()
            .map(|r| {
                format!(
                    "{} {} {}",
                    self.fields[r.field].name,
                    r.operator.label(),
                    r.value
                )
            })
            .collect();
        let joiner = if self.matching == RuleMatch::All {
            " and "
        } else {
            " or "
        };
        node.set_label("Rules");
        node.set_value(rules.join(joiner));
    }

    fn removed(&mut self, text: &mut TextStore) {
        for row in &mut self.rows {
            row.value.edit.removed(text);
        }
        for layout in self.retired.drain(..) {
            text.remove(layout);
        }
        for label in &mut self.labels {
            label.release(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(value: String) -> Rule {
        Rule {
            field: 0,
            operator: RuleOperator::Contains,
            value,
        }
    }

    #[test]
    fn unpainted_replacements_only_retain_existing_layouts() {
        let mut editor = RuleEditor::new(vec![RuleField::text("Name")], 500.0);
        for i in 0..128 {
            editor = editor.rules_from([rule(format!("builder {i}"))]);
            editor.set_rules(&mut UpdateCx::default(), vec![rule(format!("update {i}"))]);
            assert!(
                editor.retired.is_empty(),
                "rows never painted have no resources to retain"
            );
        }

        let mut text = TextStore::new(noa_gpui_text::FontOptions {
            load_system_fonts: false,
            ..Default::default()
        });
        editor.rows[0].value.measure(&mut text);
        assert_eq!(text.layouts().len(), 1);
        for i in 0..128 {
            editor.set_rules(&mut UpdateCx::default(), vec![rule(format!("hidden {i}"))]);
            assert_eq!(
                editor.retired.len(),
                1,
                "only the previously painted layout awaits release"
            );
        }
        editor.removed(&mut text);
        assert_eq!(text.layouts().len(), 0);
    }
}
