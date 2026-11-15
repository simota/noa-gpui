//! A typed, CSS-like styling layer for the retained widget tree. Only a diagnosed
//! subset of CSS is supported, and a missing declaration never overwrites a widget's
//! native defaults.

use std::collections::HashMap;
use std::fmt;

use noa_gpui_layout::Style;
use noa_gpui_layout::style::{
    self, AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, JustifyContent, Overflow,
    Position,
};
use noa_gpui_scene::Color;
use noa_gpui_text::TextAlign;

use crate::theme::{Paint, Role};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleError {
    pub message: String,
}

impl StyleError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for StyleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for StyleError {}

/// Belongs to the tree node, not to a wrapper widget, so calls such as
/// `UiTree::update::<Button>` keep working.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleMeta {
    id: Option<String>,
    classes: Vec<String>,
}

impl StyleMeta {
    pub const EMPTY: Self = Self {
        id: None,
        classes: Vec::new(),
    };

    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        let class = class.into();
        if !self.classes.contains(&class) {
            self.classes.push(class);
        }
        self
    }

    pub fn id_value(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn classes(&self) -> &[String] {
        &self.classes
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StyleState {
    pub hovered: bool,
    pub focused: bool,
    pub focus_within: bool,
    /// Focused, and the focus did not come from a pointer press.
    pub focus_visible: bool,
    /// The pointer is pressed on the node or one of its descendants.
    pub active: bool,
    pub disabled: bool,
    pub root: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct StyleNode<'a> {
    pub widget_type: &'a str,
    pub meta: &'a StyleMeta,
    pub state: StyleState,
    /// Position among the parent's children, from 0.
    pub index: usize,
    /// How many children the parent has (1 for a node without a parent).
    pub sibling_count: usize,
}

pub trait StyleTree {
    type Id: Copy;

    fn style_node(&self, id: Self::Id) -> StyleNode<'_>;
    fn style_parent(&self, id: Self::Id) -> Option<Self::Id>;
    fn style_previous_sibling(&self, id: Self::Id) -> Option<Self::Id>;
}

/// A node followed by its ancestors up to the root; nobody has siblings.
impl StyleTree for [StyleNode<'_>] {
    type Id = usize;

    fn style_node(&self, id: usize) -> StyleNode<'_> {
        self[id]
    }

    fn style_parent(&self, id: usize) -> Option<usize> {
        (id + 1 < self.len()).then_some(id + 1)
    }

    fn style_previous_sibling(&self, _id: usize) -> Option<usize> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
    Auto,
}

impl Length {
    fn dimension(self) -> style::Dimension {
        match self {
            Self::Px(value) => style::length(value),
            Self::Percent(value) => style::percent(value),
            Self::Auto => style::auto(),
        }
    }

    /// Padding has no `auto`; the parser never produces it here.
    fn padding(self) -> style::LengthPercentage {
        match self {
            Self::Px(value) => style::length(value),
            Self::Percent(value) => style::percent(value),
            Self::Auto => style::length(0.0),
        }
    }

    fn margin(self) -> style::LengthPercentageAuto {
        match self {
            Self::Px(value) => style::length(value),
            Self::Percent(value) => style::percent(value),
            Self::Auto => style::auto(),
        }
    }
}

/// Only specified properties are Some. Longhand edges use CSS order:
/// [top, right, bottom, left]. Missing properties preserve native widget styling.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StylePatch {
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_width: Option<Length>,
    pub max_height: Option<Length>,
    pub margin: [Option<Length>; 4],
    pub padding: [Option<Length>; 4],
    pub display: Option<Display>,
    pub overflow: Option<Overflow>,
    /// 0 to 1, for the node and its subtree.
    pub opacity: Option<f32>,
    pub position: Option<Position>,
    /// `top`, `right`, `bottom`, `left`.
    pub inset: [Option<Length>; 4],
    pub row_gap: Option<f32>,
    pub column_gap: Option<f32>,
    pub flex_direction: Option<FlexDirection>,
    pub flex_wrap: Option<FlexWrap>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Length>,
    pub align_items: Option<AlignItems>,
    pub align_self: Option<AlignSelf>,
    pub justify_content: Option<JustifyContent>,
    pub background_color: Option<Paint>,
    pub color: Option<Paint>,
    pub border_width: Option<f32>,
    pub border_color: Option<Paint>,
    pub border_radius: Option<f32>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub text_align: Option<TextAlign>,
    /// The text properties above that [`StylePatch::inherit_from`] took from the
    /// parent, one [`TextProperty::bit`] each.
    inherited: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextProperty {
    Color,
    FontSize,
    FontWeight,
    TextAlign,
}

impl TextProperty {
    pub(crate) const fn bit(self) -> u8 {
        1 << self as u8
    }
}

impl StylePatch {
    pub const EMPTY: Self = Self {
        width: None,
        height: None,
        min_width: None,
        min_height: None,
        max_width: None,
        max_height: None,
        margin: [None; 4],
        padding: [None; 4],
        display: None,
        overflow: None,
        opacity: None,
        position: None,
        inset: [None; 4],
        row_gap: None,
        column_gap: None,
        flex_direction: None,
        flex_wrap: None,
        flex_grow: None,
        flex_shrink: None,
        flex_basis: None,
        align_items: None,
        align_self: None,
        justify_content: None,
        background_color: None,
        color: None,
        border_width: None,
        border_color: None,
        border_radius: None,
        font_size: None,
        font_weight: None,
        text_align: None,
        inherited: 0,
    };

    pub fn is_empty(&self) -> bool {
        self == &Self::EMPTY
    }

    /// Overlay only explicitly declared values. This happens independently for each
    /// longhand, so padding shorthand followed by padding-left works as in CSS.
    pub fn overlay(&mut self, other: &Self) {
        macro_rules! set {
            ($($field:ident),* $(,)?) => {
                $(if let Some(value) = other.$field {
                    self.$field = Some(value);
                })*
            };
        }
        set!(
            width,
            height,
            min_width,
            min_height,
            max_width,
            max_height,
            position,
            display,
            overflow,
            opacity,
            row_gap,
            column_gap,
            flex_direction,
            flex_wrap,
            flex_grow,
            flex_shrink,
            flex_basis,
            align_items,
            align_self,
            justify_content,
            background_color,
            color,
            border_width,
            border_color,
            border_radius,
            font_size,
            font_weight,
            text_align
        );
        for i in 0..4 {
            if let Some(value) = other.inset[i] {
                self.inset[i] = Some(value);
            }
            if let Some(value) = other.margin[i] {
                self.margin[i] = Some(value);
            }
            if let Some(value) = other.padding[i] {
                self.padding[i] = Some(value);
            }
        }
    }

    /// Whether the two differ in anything but the paint-only colors, radius and
    /// opacity.
    pub fn affects_layout_differently(&self, other: &Self) -> bool {
        let layout_part = |patch: &Self| Self {
            background_color: None,
            color: None,
            border_color: None,
            border_radius: None,
            opacity: None,
            inherited: patch.inherited & !TextProperty::Color.bit(),
            ..patch.clone()
        };
        layout_part(self) != layout_part(other)
    }

    pub fn is_display_none(&self) -> bool {
        self.display == Some(Display::None)
    }

    pub fn clips(&self) -> bool {
        matches!(self.overflow, Some(Overflow::Hidden | Overflow::Clip))
    }

    /// Take the text properties (`color`, `font-size`, `font-weight`, `text-align`)
    /// that this node does not declare from its parent's computed style.
    pub fn inherit_from(&mut self, parent: &Self) {
        fn take<T: Copy>(own: &mut Option<T>, parent: Option<T>, bit: u8, inherited: &mut u8) {
            if own.is_none() && parent.is_some() {
                *own = parent;
                *inherited |= bit;
            }
        }
        let bits = &mut self.inherited;
        take(
            &mut self.color,
            parent.color,
            TextProperty::Color.bit(),
            bits,
        );
        take(
            &mut self.font_size,
            parent.font_size,
            TextProperty::FontSize.bit(),
            bits,
        );
        take(
            &mut self.font_weight,
            parent.font_weight,
            TextProperty::FontWeight.bit(),
            bits,
        );
        take(
            &mut self.text_align,
            parent.text_align,
            TextProperty::TextAlign.bit(),
            bits,
        );
    }

    /// Whether `property` came from an ancestor ([`StylePatch::inherit_from`]) rather
    /// than from a rule or inline declaration for this node. Inherited values rank below
    /// what a widget's own builder sets.
    pub fn is_inherited(&self, property: TextProperty) -> bool {
        self.inherited & property.bit() != 0
    }

    /// Whether a child would inherit something different from the two.
    pub fn inherited_differs(&self, other: &Self) -> bool {
        self.color != other.color
            || self.font_size != other.font_size
            || self.font_weight != other.font_weight
            || self.text_align != other.text_align
    }

    pub fn apply_layout(&self, style: &mut Style) {
        if let Some(display) = self.display {
            style.display = display;
        }
        if let Some(overflow) = self.overflow {
            style.overflow = style::Point {
                x: overflow,
                y: overflow,
            };
        }
        if let Some(value) = self.width {
            style.size.width = value.dimension();
        }
        if let Some(value) = self.height {
            style.size.height = value.dimension();
        }
        if let Some(value) = self.min_width {
            style.min_size.width = value.margin();
        }
        if let Some(value) = self.min_height {
            style.min_size.height = value.margin();
        }
        if let Some(value) = self.max_width {
            style.max_size.width = value.margin();
        }
        if let Some(value) = self.max_height {
            style.max_size.height = value.margin();
        }
        if let Some(position) = self.position {
            style.position = position;
        }
        for (edge, value) in self.inset.iter().enumerate() {
            if let Some(value) = value {
                match edge {
                    0 => style.inset.top = value.margin(),
                    1 => style.inset.right = value.margin(),
                    2 => style.inset.bottom = value.margin(),
                    _ => style.inset.left = value.margin(),
                }
            }
        }
        if let Some(wrap) = self.flex_wrap {
            style.flex_wrap = wrap;
        }
        if let Some(grow) = self.flex_grow {
            style.flex_grow = grow;
        }
        if let Some(shrink) = self.flex_shrink {
            style.flex_shrink = shrink;
        }
        if let Some(basis) = self.flex_basis {
            style.flex_basis = basis.dimension();
        }
        if let Some(align) = self.align_items {
            style.align_items = align;
        }
        if let Some(align) = self.align_self {
            style.align_self = Some(align);
        }
        if let Some(justify) = self.justify_content {
            style.justify_content = justify;
        }
        for (edge, value) in self.margin.iter().enumerate() {
            if let Some(value) = value {
                match edge {
                    0 => style.margin.top = value.margin(),
                    1 => style.margin.right = value.margin(),
                    2 => style.margin.bottom = value.margin(),
                    _ => style.margin.left = value.margin(),
                }
            }
        }
        for (edge, value) in self.padding.iter().enumerate() {
            if let Some(value) = value {
                match edge {
                    0 => style.padding.top = value.padding(),
                    1 => style.padding.right = value.padding(),
                    2 => style.padding.bottom = value.padding(),
                    _ => style.padding.left = value.padding(),
                }
            }
        }
        if let Some(gap) = self.row_gap {
            style.gap.height = style::length(gap);
        }
        if let Some(gap) = self.column_gap {
            style.gap.width = style::length(gap);
        }
        if let Some(direction) = self.flex_direction {
            style.flex_direction = direction;
        }
        if let Some(width) = self.border_width {
            style.border = style::Rect::length(width);
        }
    }

    /// For per-node inline styling. `!important` declarations win over the others in
    /// the same list. `var()` needs the stylesheet that defines the variables: see
    /// [`StyleSheet::parse_inline`].
    pub fn parse(source: &str) -> Result<Self, StyleError> {
        Ok(parse_declarations(&without_comments(source)?, &Vars::new(), false)?.merged())
    }
}

type Vars = HashMap<String, String>;

#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    rules: Vec<Rule>,
    vars: Vars,
    viewport: Option<(f32, f32)>,
    needs_ancestors: bool,
    needs_siblings: bool,
}

#[derive(Debug, Clone)]
struct Rule {
    selector: Selector,
    patch: StylePatch,
    important: StylePatch,
    media: Option<Media>,
}

impl Rule {
    fn applies<T: StyleTree + ?Sized>(
        &self,
        viewport: Option<(f32, f32)>,
        node: &StyleNode,
        tree: &T,
        id: T::Id,
    ) -> bool {
        self.media.is_none_or(|media| media.matches(viewport))
            && self.selector.matches(node, tree, id)
    }
}

/// The condition of an `@media` block: every given bound must hold.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Media {
    min_width: Option<f32>,
    max_width: Option<f32>,
    min_height: Option<f32>,
    max_height: Option<f32>,
}

impl Media {
    fn parse(query: &str) -> Result<Self, StyleError> {
        let mut media = Self::default();
        for part in query.split(" and ") {
            let feature = part
                .trim()
                .strip_prefix('(')
                .and_then(|part| part.strip_suffix(')'))
                .and_then(|part| part.split_once(':'));
            let Some((name, value)) = feature else {
                return Err(StyleError::new(format!(
                    "unsupported media query: {}",
                    query.trim()
                )));
            };
            let value = parse_pixels(value.trim())?;
            // A bound given twice keeps the stricter value: all of them must hold.
            let at_least = |bound: &mut Option<f32>| {
                *bound = Some(bound.map_or(value, |old| old.max(value)));
            };
            let at_most = |bound: &mut Option<f32>| {
                *bound = Some(bound.map_or(value, |old| old.min(value)));
            };
            match name.trim() {
                "min-width" => at_least(&mut media.min_width),
                "max-width" => at_most(&mut media.max_width),
                "min-height" => at_least(&mut media.min_height),
                "max-height" => at_most(&mut media.max_height),
                name => {
                    return Err(StyleError::new(format!(
                        "unsupported media feature: {name}"
                    )));
                }
            }
        }
        Ok(media)
    }

    /// Nothing matches until the viewport is known.
    fn matches(&self, viewport: Option<(f32, f32)>) -> bool {
        let Some((width, height)) = viewport else {
            return false;
        };
        self.min_width.is_none_or(|bound| width >= bound)
            && self.max_width.is_none_or(|bound| width <= bound)
            && self.min_height.is_none_or(|bound| height >= bound)
            && self.max_height.is_none_or(|bound| height <= bound)
    }
}

type Block<'a> = (Vec<Selector>, &'a str, Option<Media>);

fn collect_blocks<'a>(
    source: &'a str,
    media: Option<Media>,
    blocks: &mut Vec<Block<'a>>,
    vars: &mut Vars,
) -> Result<(), StyleError> {
    let mut remaining = source;
    while !remaining.trim().is_empty() {
        let (head, after_open) = remaining
            .split_once('{')
            .ok_or_else(|| StyleError::new("expected '{' after selector"))?;
        let head = head.trim();
        if let Some(query) = head.strip_prefix("@media") {
            if media.is_some() {
                return Err(StyleError::new("nested @media is not supported"));
            }
            let mut level = 1;
            let end = after_open
                .char_indices()
                .find_map(|(i, c)| {
                    match c {
                        '{' => level += 1,
                        '}' => level -= 1,
                        _ => {}
                    }
                    (level == 0).then_some(i)
                })
                .ok_or_else(|| StyleError::new("unclosed @media block"))?;
            collect_blocks(&after_open[..end], Some(Media::parse(query)?), blocks, vars)?;
            remaining = &after_open[end + 1..];
            continue;
        }
        if head.starts_with('@') {
            return Err(StyleError::new(format!("unsupported at-rule: {head}")));
        }
        let (body, tail) = after_open
            .split_once('}')
            .ok_or_else(|| StyleError::new("unclosed style rule"))?;
        if body.contains('{') {
            return Err(StyleError::new("nested rules are not supported"));
        }
        let selectors = split_top_level(head, ',')
            .into_iter()
            .map(Selector::parse)
            .collect::<Result<Vec<_>, _>>()?;
        // Variables are global: collect them all before any value is read, so a
        // rule may use a variable that a later `:root` block defines.
        let global = media.is_none() && selectors.iter().all(Selector::is_root_only);
        for (name, value) in declarations(body) {
            let (name, value) = (name?, value.trim());
            if name.starts_with("--") {
                if !global {
                    return Err(StyleError::new(format!(
                        "custom property {name} must be declared in :root, outside @media"
                    )));
                }
                // As in CSS, `!important` ranks the custom property itself; it is not
                // part of the value that var() copies.
                let value = value.strip_suffix("!important").unwrap_or(value).trim_end();
                vars.insert(name.to_owned(), value.to_owned());
            }
        }
        blocks.push((selectors, body, media));
        remaining = tail;
    }
    Ok(())
}

fn split_top_level(source: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut level = 0usize;
    let mut start = 0;
    for (i, c) in source.char_indices() {
        match c {
            '(' => level += 1,
            ')' => level = level.saturating_sub(1),
            c if c == separator && level == 0 => {
                parts.push(&source[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&source[start..]);
    parts
}

impl StyleSheet {
    /// A selector is made of compounds (type, `*`, #id, .class and pseudo-classes)
    /// joined by the descendant (space), child (`>`), next-sibling (`+`) and
    /// subsequent-sibling (`~`) combinators. Declarations are typed, may be
    /// `!important`, and may read custom properties (`--name`, declared in `:root`) with
    /// `var()`. Unsupported constructs are errors.
    pub fn parse(source: &str) -> Result<Self, StyleError> {
        let clean = without_comments(source)?;
        let mut blocks = Vec::new();
        let mut vars = Vars::new();
        collect_blocks(&clean, None, &mut blocks, &mut vars)?;
        let vars = resolve_vars(&vars)?;
        let mut rules = Vec::new();
        for (selectors, body, media) in blocks {
            let parsed = parse_declarations(body, &vars, true)?;
            for selector in selectors {
                rules.push(Rule {
                    selector,
                    patch: parsed.normal.clone(),
                    important: parsed.important.clone(),
                    media,
                });
            }
        }
        // Rules apply by specificity, then in source order. They are sorted once here
        // (stably), and resolving a node goes through them in order.
        rules.sort_by_key(|rule| rule.selector.specificity());
        Ok(Self {
            needs_ancestors: rules.iter().any(|rule| !rule.selector.chain.is_empty()),
            needs_siblings: rules.iter().any(|rule| rule.selector.needs_siblings()),
            rules,
            vars,
            viewport: None,
        })
    }

    pub fn parse_inline(&self, source: &str) -> Result<StylePatch, StyleError> {
        Ok(parse_declarations(&without_comments(source)?, &self.vars, false)?.merged())
    }

    /// Returns whether the new viewport switched any `@media` rule on or off, so that
    /// nodes have to be resolved again.
    pub fn set_viewport(&mut self, width: f32, height: f32) -> bool {
        let new = Some((width, height));
        let old = std::mem::replace(&mut self.viewport, new);
        self.rules
            .iter()
            .filter_map(|rule| rule.media)
            .any(|media| media.matches(old) != media.matches(new))
    }

    /// Resolve a node on its own, as an only child without ancestors.
    pub fn resolve(&self, widget_type: &str, meta: &StyleMeta, state: StyleState) -> StylePatch {
        self.resolve_path(
            &[StyleNode {
                widget_type,
                meta,
                state,
                index: 0,
                sibling_count: 1,
            }],
            &StylePatch::EMPTY,
        )
    }

    /// `path[0]` is the node; its ancestors follow from its parent up to the root.
    ///
    /// # Panics
    /// If `path` is empty.
    pub fn resolve_path(&self, path: &[StyleNode], inline: &StylePatch) -> StylePatch {
        self.resolve_in(path, 0, inline)
    }

    /// Precedence, lowest first: rules by specificity then source order, the node's
    /// `inline` declarations, then `!important` declarations (again by specificity).
    /// Inherited properties are the caller's to fill in
    /// ([`StylePatch::inherit_from`]).
    pub fn resolve_in<T: StyleTree + ?Sized>(
        &self,
        tree: &T,
        id: T::Id,
        inline: &StylePatch,
    ) -> StylePatch {
        // Worked out once for all the rules, since its state looks at its ancestors.
        let node = tree.style_node(id);
        let mut result = StylePatch::default();
        // Few rules have `!important` declarations, so only those are kept for later.
        let mut important = Vec::new();
        for rule in &self.rules {
            if rule.applies(self.viewport, &node, tree, id) {
                result.overlay(&rule.patch);
                if !rule.important.is_empty() {
                    important.push(&rule.important);
                }
            }
        }
        result.overlay(inline);
        for patch in important {
            result.overlay(patch);
        }
        result
    }

    /// Whether any selector looks at other nodes (ancestors or siblings), so that a
    /// change to a node can restyle its descendants.
    pub fn needs_ancestors(&self) -> bool {
        self.needs_ancestors
    }

    /// Whether any selector depends on a node's siblings or its position among them,
    /// so that a change to a node or to a list of children can restyle the siblings.
    pub fn needs_siblings(&self) -> bool {
        self.needs_siblings
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Combinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}

#[derive(Debug, Clone)]
struct Selector {
    subject: Compound,
    /// The compounds to the left of the subject, nearest first. The combinator
    /// relates the compound to the one after it in source order (the subject, or the
    /// previous entry here).
    chain: Vec<(Combinator, Compound)>,
}

impl Selector {
    fn parse(raw: &str) -> Result<Self, StyleError> {
        let misplaced = || StyleError::new(format!("misplaced combinator in selector: {raw}"));
        let mut compounds = Vec::new();
        let mut combinator = None;
        let mut level = 0usize;
        let mut start = None;
        for (i, c) in raw.char_indices().chain([(raw.len(), ' ')]) {
            let separates = level == 0 && (c.is_whitespace() || matches!(c, '>' | '+' | '~'));
            if !separates {
                match c {
                    '(' => level += 1,
                    ')' => level = level.saturating_sub(1),
                    _ => {}
                }
                start.get_or_insert(i);
                continue;
            }
            if let Some(from) = start.take() {
                let relation = combinator.take().unwrap_or(Combinator::Descendant);
                compounds.push((relation, Compound::parse(&raw[from..i])?));
            }
            let explicit = match c {
                '>' => Combinator::Child,
                '+' => Combinator::NextSibling,
                '~' => Combinator::SubsequentSibling,
                _ => continue,
            };
            if compounds.is_empty() || combinator.replace(explicit).is_some() {
                return Err(misplaced());
            }
        }
        if level != 0 {
            return Err(StyleError::new(format!("unclosed '(' in selector: {raw}")));
        }
        if combinator.is_some() {
            return Err(misplaced());
        }
        let mut chain = Vec::new();
        let (mut relation, subject) = compounds
            .pop()
            .ok_or_else(|| StyleError::new("empty selector"))?;
        while let Some((next_relation, compound)) = compounds.pop() {
            chain.push((relation, compound));
            relation = next_relation;
        }
        Ok(Self { subject, chain })
    }

    fn is_root_only(&self) -> bool {
        self.chain.is_empty()
            && self.subject
                == Compound {
                    root: true,
                    ..Default::default()
                }
    }

    fn needs_siblings(&self) -> bool {
        self.subject.is_positional()
            || self.chain.iter().any(|(combinator, compound)| {
                compound.is_positional()
                    || matches!(
                        combinator,
                        Combinator::NextSibling | Combinator::SubsequentSibling
                    )
            })
    }

    fn specificity(&self) -> (usize, usize, usize) {
        let mut total = self.subject.specificity();
        for (_, compound) in &self.chain {
            let (ids, classes, types) = compound.specificity();
            total = (total.0 + ids, total.1 + classes, total.2 + types);
        }
        total
    }

    /// Whether the selector matches the node `id` of `tree`, which is `node`.
    fn matches<T: StyleTree + ?Sized>(&self, node: &StyleNode, tree: &T, id: T::Id) -> bool {
        self.subject.matches(node) && Self::match_chain(&self.chain, tree, id)
    }

    /// Whether the compounds in `rest` are found from the node `at`, which matched
    /// the compound after them.
    fn match_chain<T: StyleTree + ?Sized>(
        rest: &[(Combinator, Compound)],
        tree: &T,
        at: T::Id,
    ) -> bool {
        let Some(((combinator, compound), tail)) = rest.split_first() else {
            return true;
        };
        let step = |id: T::Id| match combinator {
            Combinator::Descendant | Combinator::Child => tree.style_parent(id),
            Combinator::NextSibling | Combinator::SubsequentSibling => {
                tree.style_previous_sibling(id)
            }
        };
        let once = matches!(combinator, Combinator::Child | Combinator::NextSibling);
        let mut candidate = step(at);
        while let Some(id) = candidate {
            if compound.matches(&tree.style_node(id)) && Self::match_chain(tail, tree, id) {
                return true;
            }
            if once {
                return false;
            }
            candidate = step(id);
        }
        false
    }
}

/// `:nth-child(an+b)`, counted from the end for `:nth-last-child`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Nth {
    a: i32,
    b: i32,
    from_end: bool,
}

impl Nth {
    const FIRST: Self = Self {
        a: 0,
        b: 1,
        from_end: false,
    };
    const LAST: Self = Self {
        a: 0,
        b: 1,
        from_end: true,
    };

    fn parse(argument: &str, from_end: bool) -> Result<Self, StyleError> {
        let invalid = || StyleError::new(format!("invalid nth-child argument: {argument}"));
        let compact: String = argument.chars().filter(|c| !c.is_whitespace()).collect();
        let number = |text: &str| text.parse::<i32>().map_err(|_| invalid());
        let (a, b) = match compact.as_str() {
            "odd" => (2, 1),
            "even" => (2, 0),
            text => match text.split_once('n') {
                Some((a, b)) => (
                    match a {
                        "" | "+" => 1,
                        "-" => -1,
                        a => number(a)?,
                    },
                    // `2n3` is not `2n+3`: after `n` comes nothing or a sign.
                    match b.as_bytes().first() {
                        None => 0,
                        Some(b'+' | b'-') => number(b)?,
                        Some(_) => return Err(invalid()),
                    },
                ),
                None => (0, number(text)?),
            },
        };
        Ok(Self { a, b, from_end })
    }

    fn matches(&self, node: &StyleNode) -> bool {
        let position = if self.from_end {
            node.sibling_count.saturating_sub(node.index)
        } else {
            node.index + 1
        };
        // In 64 bits: any accepted `a` and `b` can be subtracted and divided.
        let (a, offset) = (i64::from(self.a), position as i64 - i64::from(self.b));
        if a == 0 {
            offset == 0
        } else {
            offset % a == 0 && offset / a >= 0
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Compound {
    widget_type: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    hovered: bool,
    focused: bool,
    focus_within: bool,
    focus_visible: bool,
    active: bool,
    disabled: bool,
    root: bool,
    only_child: bool,
    nth: Vec<Nth>,
    not: Vec<Compound>,
}

impl Compound {
    fn parse(raw: &str) -> Result<Self, StyleError> {
        let mut rest = raw.trim();
        if rest.is_empty() {
            return Err(StyleError::new("empty selector"));
        }
        let mut selector = Self::default();
        if let Some(after) = rest.strip_prefix('*') {
            rest = after;
        } else {
            let count = identifier_length(rest);
            if count > 0 {
                selector.widget_type = Some(rest[..count].to_owned());
                rest = &rest[count..];
            }
        }
        while !rest.is_empty() {
            let kind = rest
                .chars()
                .next()
                .ok_or_else(|| StyleError::new("empty selector"))?;
            if !matches!(kind, '.' | '#' | ':') {
                return Err(StyleError::new(format!(
                    "unsupported selector syntax: {raw}"
                )));
            }
            rest = &rest[1..];
            let count = identifier_length(rest);
            if count == 0 {
                return Err(StyleError::new(format!("missing selector name in {raw}")));
            }
            let name = &rest[..count];
            rest = &rest[count..];
            match kind {
                '.' => selector.classes.push(name.to_owned()),
                '#' => {
                    if selector.id.replace(name.to_owned()).is_some() {
                        return Err(StyleError::new("selector has multiple IDs"));
                    }
                }
                ':' => {
                    let argument = match rest.strip_prefix('(') {
                        Some(after) => {
                            let mut level = 1;
                            let end = after
                                .char_indices()
                                .find_map(|(i, c)| {
                                    match c {
                                        '(' => level += 1,
                                        ')' => level -= 1,
                                        _ => {}
                                    }
                                    (level == 0).then_some(i)
                                })
                                .ok_or_else(|| StyleError::new(format!("unclosed '(' in {raw}")))?;
                            rest = &after[end + 1..];
                            Some(after[..end].trim())
                        }
                        None => None,
                    };
                    match (name, argument) {
                        ("hover", None) => selector.hovered = true,
                        ("focus", None) => selector.focused = true,
                        ("focus-within", None) => selector.focus_within = true,
                        ("focus-visible", None) => selector.focus_visible = true,
                        ("active", None) => selector.active = true,
                        ("disabled", None) => selector.disabled = true,
                        ("root", None) => selector.root = true,
                        ("first-child", None) => selector.nth.push(Nth::FIRST),
                        ("last-child", None) => selector.nth.push(Nth::LAST),
                        ("only-child", None) => selector.only_child = true,
                        ("nth-child", Some(argument)) => {
                            selector.nth.push(Nth::parse(argument, false)?);
                        }
                        ("nth-last-child", Some(argument)) => {
                            selector.nth.push(Nth::parse(argument, true)?);
                        }
                        ("not", Some(argument)) => selector.not.push(Self::parse(argument)?),
                        _ => {
                            return Err(StyleError::new(format!(
                                "unsupported pseudo-class :{name}"
                            )));
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        Ok(selector)
    }

    fn is_positional(&self) -> bool {
        self.only_child || !self.nth.is_empty() || self.not.iter().any(Self::is_positional)
    }

    fn specificity(&self) -> (usize, usize, usize) {
        let mut total = (
            usize::from(self.id.is_some()),
            self.classes.len()
                + self.nth.len()
                + usize::from(self.hovered)
                + usize::from(self.focused)
                + usize::from(self.focus_within)
                + usize::from(self.focus_visible)
                + usize::from(self.active)
                + usize::from(self.disabled)
                + usize::from(self.root)
                + usize::from(self.only_child),
            usize::from(self.widget_type.is_some()),
        );
        for inner in &self.not {
            let (ids, classes, types) = inner.specificity();
            total = (total.0 + ids, total.1 + classes, total.2 + types);
        }
        total
    }

    fn matches(&self, node: &StyleNode) -> bool {
        let (meta, state) = (node.meta, node.state);
        self.widget_type
            .as_deref()
            .is_none_or(|expected| expected == node.widget_type)
            && self
                .id
                .as_deref()
                .is_none_or(|expected| meta.id_value() == Some(expected))
            && self
                .classes
                .iter()
                .all(|class| meta.classes.contains(class))
            && (!self.hovered || state.hovered)
            && (!self.focused || state.focused)
            && (!self.focus_within || state.focus_within)
            && (!self.focus_visible || state.focus_visible)
            && (!self.active || state.active)
            && (!self.disabled || state.disabled)
            && (!self.root || state.root)
            && (!self.only_child || (node.index == 0 && node.sibling_count <= 1))
            && self.nth.iter().all(|nth| nth.matches(node))
            && !self.not.iter().any(|inner| inner.matches(node))
    }
}

fn identifier_length(input: &str) -> usize {
    input
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(*b, b'_' | b'-'))
        .count()
}

fn without_comments(input: &str) -> Result<String, StyleError> {
    let mut remaining = input;
    let mut out = String::new();
    while let Some(start) = remaining.find("/*") {
        out.push_str(&remaining[..start]);
        let after = &remaining[start + 2..];
        let end = after
            .find("*/")
            .ok_or_else(|| StyleError::new("unclosed CSS comment"))?;
        remaining = &after[end + 2..];
    }
    out.push_str(remaining);
    Ok(out)
}

#[derive(Default)]
struct Declarations {
    normal: StylePatch,
    important: StylePatch,
}

impl Declarations {
    fn merged(mut self) -> StylePatch {
        self.normal.overlay(&self.important);
        self.normal
    }
}

/// The `name: value` pairs of a declaration list; a name is an error when its ':' is
/// missing.
fn declarations(source: &str) -> impl Iterator<Item = (Result<&str, StyleError>, &str)> {
    source
        .split(';')
        .map(str::trim)
        .filter(|declaration| !declaration.is_empty())
        .map(|declaration| match declaration.split_once(':') {
            Some((name, value)) => (Ok(name.trim()), value),
            None => (
                Err(StyleError::new(format!("expected ':' in {declaration}"))),
                "",
            ),
        })
}

/// `in_sheet` is for a rule of a stylesheet, whose custom properties
/// [`collect_blocks`] has already checked and collected; anywhere else they are errors.
fn parse_declarations(
    source: &str,
    vars: &Vars,
    in_sheet: bool,
) -> Result<Declarations, StyleError> {
    let mut result = Declarations::default();
    for (name, value) in declarations(source) {
        let name = name?;
        if name.starts_with("--") {
            if in_sheet {
                continue;
            }
            return Err(StyleError::new(format!(
                "custom property {name} must be declared in :root of a stylesheet"
            )));
        }
        // `!important` belongs to this declaration, not to the variables it uses.
        let (value, important) = match value.trim().strip_suffix("!important") {
            Some(value) => (value, true),
            None => (value, false),
        };
        let value = substitute(value, 0, &mut |name| Ok(vars.get(name).cloned()))?;
        let target = if important {
            &mut result.important
        } else {
            &mut result.normal
        };
        set_property(target, name, value.trim())?;
    }
    Ok(result)
}

/// The longest value `var()` may expand to, so that a few variables that each use
/// the previous one many times cannot grow a stylesheet without bound.
const MAX_EXPANDED: usize = 16 * 1024;
/// How deep variables may refer to variables that refer to variables.
const MAX_VAR_DEPTH: usize = 64;

/// Expand the `var()` references in every variable's value, each variable once, so
/// that using them costs no more than copying them. An unknown variable, a cycle or
/// an expansion past the limits is an error whether or not a rule uses the variable.
fn resolve_vars(raw: &Vars) -> Result<Vars, StyleError> {
    let mut names: Vec<&String> = raw.keys().collect();
    names.sort();
    let mut done = Vars::new();
    for name in names {
        resolve_var(name, raw, &mut done, &mut Vec::new())?;
    }
    Ok(done)
}

/// `name`'s value with its references expanded, or `None` if it is not declared.
fn resolve_var(
    name: &str,
    raw: &Vars,
    done: &mut Vars,
    visiting: &mut Vec<String>,
) -> Result<Option<String>, StyleError> {
    if let Some(value) = done.get(name) {
        return Ok(Some(value.clone()));
    }
    let Some(value) = raw.get(name) else {
        return Ok(None);
    };
    if visiting.iter().any(|v| v == name) {
        visiting.push(name.to_owned());
        return Err(StyleError::new(format!(
            "variables refer to each other in a cycle: {}",
            visiting.join(" -> ")
        )));
    }
    if visiting.len() >= MAX_VAR_DEPTH {
        return Err(StyleError::new(format!(
            "variables nest more than {MAX_VAR_DEPTH} deep at {name}"
        )));
    }
    visiting.push(name.to_owned());
    let expanded = substitute(value, 0, &mut |n| resolve_var(n, raw, done, visiting))?;
    visiting.pop();
    done.insert(name.to_owned(), expanded.clone());
    Ok(Some(expanded))
}

/// Replace every `var(--name)` / `var(--name, fallback)` with what `lookup` gives for
/// the name, or else the fallback. `depth` counts the fallbacks this value is nested
/// in, which are limited like variables so that no input can exhaust the stack.
fn substitute(
    value: &str,
    depth: usize,
    lookup: &mut dyn FnMut(&str) -> Result<Option<String>, StyleError>,
) -> Result<String, StyleError> {
    if depth >= MAX_VAR_DEPTH {
        return Err(StyleError::new(format!(
            "var() fallbacks nest more than {MAX_VAR_DEPTH} deep"
        )));
    }
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("var(") {
        out.push_str(&rest[..start]);
        let inner_start = start + 4;
        let mut level = 1;
        let mut end = None;
        for (offset, c) in rest[inner_start..].char_indices() {
            match c {
                '(' => level += 1,
                ')' => {
                    level -= 1;
                    if level == 0 {
                        end = Some(inner_start + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end.ok_or_else(|| StyleError::new(format!("unclosed var() in {value}")))?;
        let inner = &rest[inner_start..end];
        let (name, fallback) = match inner.split_once(',') {
            Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
            None => (inner.trim(), None),
        };
        if !name.starts_with("--") {
            return Err(StyleError::new(format!("var() expects a --name: {inner}")));
        }
        match (lookup(name)?, fallback) {
            (Some(value), _) => out.push_str(&value),
            (None, Some(fallback)) => out.push_str(&substitute(fallback, depth + 1, lookup)?),
            (None, None) => return Err(StyleError::new(format!("unknown variable: {name}"))),
        }
        if out.len() > MAX_EXPANDED {
            return Err(StyleError::new(format!(
                "var() expands to more than {MAX_EXPANDED} bytes in {value}"
            )));
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Split a value at whitespace outside parentheses: `1px solid rgb(0, 0, 0)` is three.
fn split_values(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut level = 0usize;
    let mut start = None;
    for (i, c) in value.char_indices() {
        match c {
            '(' => level += 1,
            ')' => level = level.saturating_sub(1),
            _ => {}
        }
        if c.is_whitespace() && level == 0 {
            if let Some(from) = start.take() {
                parts.push(&value[from..i]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(from) = start {
        parts.push(&value[from..]);
    }
    parts
}

fn set_property(result: &mut StylePatch, name: &str, value: &str) -> Result<(), StyleError> {
    let keyword =
        |options: &str| StyleError::new(format!("invalid {name}: {value} (expected {options})"));
    match name {
        "width" => result.width = Some(parse_size(value)?),
        "height" => result.height = Some(parse_size(value)?),
        "min-width" => result.min_width = Some(parse_size(value)?),
        "min-height" => result.min_height = Some(parse_size(value)?),
        "max-width" => result.max_width = Some(parse_max(value)?),
        "max-height" => result.max_height = Some(parse_max(value)?),
        "padding" => result.padding = shorthand_edges(value, parse_length)?,
        "margin" => result.margin = shorthand_edges(value, parse_offset)?,
        "inset" => result.inset = shorthand_edges(value, parse_offset)?,
        "padding-top" => result.padding[0] = Some(parse_length(value)?),
        "padding-right" => result.padding[1] = Some(parse_length(value)?),
        "padding-bottom" => result.padding[2] = Some(parse_length(value)?),
        "padding-left" => result.padding[3] = Some(parse_length(value)?),
        "margin-top" => result.margin[0] = Some(parse_offset(value)?),
        "margin-right" => result.margin[1] = Some(parse_offset(value)?),
        "margin-bottom" => result.margin[2] = Some(parse_offset(value)?),
        "margin-left" => result.margin[3] = Some(parse_offset(value)?),
        "top" => result.inset[0] = Some(parse_offset(value)?),
        "right" => result.inset[1] = Some(parse_offset(value)?),
        "bottom" => result.inset[2] = Some(parse_offset(value)?),
        "left" => result.inset[3] = Some(parse_offset(value)?),
        "display" => {
            result.display = Some(match value {
                "flex" => Display::Flex,
                "block" => Display::Block,
                "none" => Display::None,
                _ => return Err(keyword("flex, block or none")),
            });
        }
        "overflow" => {
            result.overflow = Some(match value {
                "visible" => Overflow::Visible,
                "hidden" => Overflow::Hidden,
                "clip" => Overflow::Clip,
                _ => return Err(keyword("visible, hidden or clip")),
            });
        }
        "opacity" => {
            let opacity = match value.strip_suffix('%') {
                Some(percent) => parse_number(percent)? / 100.0,
                None => parse_number(value)?,
            };
            if !(0.0..=1.0).contains(&opacity) {
                return Err(keyword("0 to 1 or 0% to 100%"));
            }
            result.opacity = Some(opacity);
        }
        "position" => {
            result.position = Some(match value {
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                _ => return Err(keyword("relative or absolute")),
            });
        }
        "gap" => match split_values(value).as_slice() {
            [gap] => {
                let gap = parse_pixels(gap)?;
                result.row_gap = Some(gap);
                result.column_gap = Some(gap);
            }
            [row, column] => {
                result.row_gap = Some(parse_pixels(row)?);
                result.column_gap = Some(parse_pixels(column)?);
            }
            _ => return Err(StyleError::new("gap expects one or two lengths")),
        },
        "row-gap" => result.row_gap = Some(parse_pixels(value)?),
        "column-gap" => result.column_gap = Some(parse_pixels(value)?),
        "flex-direction" => {
            result.flex_direction = Some(match value {
                "row" => FlexDirection::Row,
                "column" => FlexDirection::Column,
                "row-reverse" => FlexDirection::RowReverse,
                "column-reverse" => FlexDirection::ColumnReverse,
                _ => return Err(StyleError::new(format!("invalid flex-direction: {value}"))),
            });
        }
        "flex-wrap" => {
            result.flex_wrap = Some(match value {
                "nowrap" => FlexWrap::NoWrap,
                "wrap" => FlexWrap::Wrap,
                "wrap-reverse" => FlexWrap::WrapReverse,
                _ => return Err(keyword("nowrap, wrap or wrap-reverse")),
            });
        }
        "flex-grow" => result.flex_grow = Some(parse_factor(value)?),
        "flex-shrink" => result.flex_shrink = Some(parse_factor(value)?),
        "flex-basis" => result.flex_basis = Some(parse_size(value)?),
        "flex" => {
            let (grow, shrink, basis) = match split_values(value).as_slice() {
                ["none"] => (0.0, 0.0, Length::Auto),
                [grow] => (parse_factor(grow)?, 1.0, Length::Px(0.0)),
                [grow, shrink] => (parse_factor(grow)?, parse_factor(shrink)?, Length::Px(0.0)),
                [grow, shrink, basis] => (
                    parse_factor(grow)?,
                    parse_factor(shrink)?,
                    parse_size(basis)?,
                ),
                _ => return Err(keyword("none or '<grow> [<shrink> [<basis>]]'")),
            };
            result.flex_grow = Some(grow);
            result.flex_shrink = Some(shrink);
            result.flex_basis = Some(basis);
        }
        "align-items" => {
            result.align_items = Some(parse_align(value).ok_or_else(|| {
                keyword("start, end, flex-start, flex-end, center, baseline or stretch")
            })?);
        }
        "align-self" => {
            result.align_self = Some(parse_align(value).ok_or_else(|| {
                keyword("start, end, flex-start, flex-end, center, baseline or stretch")
            })?);
        }
        "justify-content" => {
            result.justify_content = Some(match value {
                "start" => JustifyContent::START,
                "end" => JustifyContent::END,
                "flex-start" => JustifyContent::FLEX_START,
                "flex-end" => JustifyContent::FLEX_END,
                "center" => JustifyContent::CENTER,
                "stretch" => JustifyContent::STRETCH,
                "space-between" => JustifyContent::SPACE_BETWEEN,
                "space-around" => JustifyContent::SPACE_AROUND,
                "space-evenly" => JustifyContent::SPACE_EVENLY,
                _ => {
                    return Err(keyword(
                        "start, end, flex-start, flex-end, center, stretch, \
                         space-between, space-around or space-evenly",
                    ));
                }
            });
        }
        "background-color" => result.background_color = Some(parse_color(value)?),
        "color" => result.color = Some(parse_color(value)?),
        "border-width" => result.border_width = Some(parse_pixels(value)?),
        "border-color" => result.border_color = Some(parse_color(value)?),
        "border-radius" => result.border_radius = Some(parse_pixels(value)?),
        "font-size" => {
            let size = parse_pixels(value)?;
            if size <= 0.0 {
                return Err(StyleError::new("font-size must be positive"));
            }
            result.font_size = Some(size);
        }
        "font-weight" => {
            result.font_weight = Some(match value {
                "normal" => 400,
                "bold" => 700,
                _ => match value.parse::<u16>() {
                    Ok(weight) if (1..=1000).contains(&weight) => weight,
                    _ => return Err(keyword("normal, bold or 1 to 1000")),
                },
            });
        }
        "text-align" => {
            result.text_align = Some(match value {
                "start" | "left" => TextAlign::Start,
                "center" => TextAlign::Center,
                "end" | "right" => TextAlign::End,
                _ => return Err(keyword("start, left, center, end or right")),
            });
        }
        "border" if value == "none" => result.border_width = Some(0.0),
        "border" => match split_values(value).as_slice() {
            [width, "solid", color] => {
                result.border_width = Some(parse_pixels(width)?);
                result.border_color = Some(parse_color(color)?);
            }
            _ => {
                return Err(StyleError::new(
                    "border expects '<width> solid <color>' or none",
                ));
            }
        },
        _ => {
            return Err(StyleError::new(format!(
                "unsupported style property: {name}"
            )));
        }
    }
    Ok(())
}

fn parse_align(value: &str) -> Option<AlignItems> {
    Some(match value {
        "start" => AlignItems::START,
        "end" => AlignItems::END,
        "flex-start" => AlignItems::FLEX_START,
        "flex-end" => AlignItems::FLEX_END,
        "center" => AlignItems::CENTER,
        "baseline" => AlignItems::BASELINE,
        "stretch" => AlignItems::STRETCH,
        _ => return None,
    })
}

fn shorthand_edges(
    value: &str,
    parse: fn(&str) -> Result<Length, StyleError>,
) -> Result<[Option<Length>; 4], StyleError> {
    let parts: Vec<Length> = split_values(value)
        .into_iter()
        .map(parse)
        .collect::<Result<_, _>>()?;
    let values = match parts.as_slice() {
        [a] => [*a, *a, *a, *a],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d] => [*a, *b, *c, *d],
        _ => return Err(StyleError::new("expected 1 to 4 edge values")),
    };
    Ok(values.map(Some))
}

fn parse_number(value: &str) -> Result<f32, StyleError> {
    let number = value
        .parse::<f32>()
        .map_err(|_| StyleError::new(format!("invalid number: {value}")))?;
    if !number.is_finite() {
        return Err(StyleError::new("non-finite numbers are not supported"));
    }
    Ok(number)
}

/// A non-negative, unitless flex factor.
fn parse_factor(value: &str) -> Result<f32, StyleError> {
    let number = parse_number(value)?;
    if number < 0.0 {
        return Err(StyleError::new(format!(
            "expected a non-negative number: {value}"
        )));
    }
    Ok(number)
}

/// A px or % length of either sign.
fn parse_signed(value: &str) -> Result<Length, StyleError> {
    if let Some(percent) = value.strip_suffix('%') {
        return Ok(Length::Percent(parse_number(percent)? / 100.0));
    }
    if value == "0" {
        return Ok(Length::Px(0.0));
    }
    let number = value
        .strip_suffix("px")
        .ok_or_else(|| StyleError::new(format!("expected a length in px or %: {value}")))?;
    Ok(Length::Px(parse_number(number)?))
}

/// A non-negative px or % length (padding).
fn parse_length(value: &str) -> Result<Length, StyleError> {
    match parse_signed(value)? {
        Length::Px(number) | Length::Percent(number) if number < 0.0 => Err(StyleError::new(
            format!("negative lengths are not supported here: {value}"),
        )),
        length => Ok(length),
    }
}

fn parse_pixels(value: &str) -> Result<f32, StyleError> {
    match parse_length(value)? {
        Length::Px(number) => Ok(number),
        _ => Err(StyleError::new(format!("expected a length in px: {value}"))),
    }
}

/// A size: a non-negative length or `auto`.
fn parse_size(value: &str) -> Result<Length, StyleError> {
    if value == "auto" {
        return Ok(Length::Auto);
    }
    parse_length(value)
}

/// A maximum size: a non-negative length, or `none` for no limit.
fn parse_max(value: &str) -> Result<Length, StyleError> {
    if value == "none" {
        return Ok(Length::Auto);
    }
    parse_length(value)
}

/// A margin or inset: a length of either sign, or `auto`.
fn parse_offset(value: &str) -> Result<Length, StyleError> {
    if value == "auto" {
        return Ok(Length::Auto);
    }
    parse_signed(value)
}

/// The 3 or 4 arguments of `rgb()` / `hsl()`: all separated by commas (`1, 2, 3, 0.5`),
/// or by spaces with the alpha after a slash (`1 2 3 / 0.5`). Not a mix of the two,
/// and no empty argument (`1,,2,3`).
fn color_arguments(arguments: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = if arguments.contains(',') {
        arguments.split(',').map(str::trim).collect()
    } else {
        let (channels, alpha) = match arguments.split_once('/') {
            Some((channels, alpha)) => (channels, Some(alpha.trim())),
            None => (arguments, None),
        };
        let mut parts: Vec<&str> = channels.split_whitespace().collect();
        if parts.len() != 3 {
            return None;
        }
        parts.extend(alpha);
        parts
    };
    let valid =
        |part: &&str| !part.is_empty() && !part.contains(|c: char| c == '/' || c.is_whitespace());
    (matches!(parts.len(), 3 | 4) && parts.iter().all(valid)).then_some(parts)
}

fn parse_color(value: &str) -> Result<Paint, StyleError> {
    let invalid = || StyleError::new(format!("invalid color: {value}"));
    if let Some(hex) = value.strip_prefix('#') {
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StyleError::new(format!("invalid hex color: {value}")));
        }
        let digits: Vec<u32> = hex.chars().filter_map(|c| c.to_digit(16)).collect();
        let channels: Vec<u32> = match digits.len() {
            3 | 4 => digits.iter().map(|d| d * 17).collect(),
            6 | 8 => digits
                .chunks(2)
                .map(|pair| pair[0] * 16 + pair[1])
                .collect(),
            _ => return Err(StyleError::new(format!("invalid hex color: {value}"))),
        };
        let rgb = (channels[0] << 16) | (channels[1] << 8) | channels[2];
        let color = Color::hex(rgb);
        return Ok(Paint::Fixed(match channels.get(3) {
            Some(alpha) => color.with_alpha(*alpha as f32 / 255.0),
            None => color,
        }));
    }
    if value == "transparent" {
        return Ok(Paint::Fixed(Color::TRANSPARENT));
    }
    let function = |names: [&str; 2]| -> Option<Result<Vec<&str>, StyleError>> {
        let arguments = names.iter().find_map(|name| value.strip_prefix(name))?;
        let Some(arguments) = arguments.strip_suffix(')') else {
            return Some(Err(invalid()));
        };
        Some(color_arguments(arguments).ok_or_else(invalid))
    };
    let alpha = |part: Option<&&str>| -> Result<f32, StyleError> {
        let alpha = match part {
            Some(part) => match part.strip_suffix('%') {
                Some(percent) => parse_number(percent)? / 100.0,
                None => parse_number(part)?,
            },
            None => 1.0,
        };
        if (0.0..=1.0).contains(&alpha) {
            Ok(alpha)
        } else {
            Err(invalid())
        }
    };
    if let Some(parts) = function(["rgba(", "rgb("]) {
        let parts = parts?;
        let mut rgb = [0.0; 3];
        for (channel, part) in rgb.iter_mut().zip(&parts) {
            let number = parse_number(part)?;
            if !(0.0..=255.0).contains(&number) {
                return Err(invalid());
            }
            *channel = number / 255.0;
        }
        let alpha = alpha(parts.get(3))?;
        return Ok(Paint::Fixed(Color::rgba(rgb[0], rgb[1], rgb[2], alpha)));
    }
    if let Some(parts) = function(["hsla(", "hsl("]) {
        let parts = parts?;
        let hue = parse_number(parts[0].strip_suffix("deg").unwrap_or(parts[0]))?;
        let percent = |part: &str| -> Result<f32, StyleError> {
            let number = parse_number(part.strip_suffix('%').ok_or_else(invalid)?)?;
            if (0.0..=100.0).contains(&number) {
                Ok(number / 100.0)
            } else {
                Err(invalid())
            }
        };
        let (saturation, lightness) = (percent(parts[1])?, percent(parts[2])?);
        // CSS Color 4, "HSL to RGB".
        let channel = |n: f32| {
            let k = (n + hue.rem_euclid(360.0) / 30.0) % 12.0;
            let a = saturation * lightness.min(1.0 - lightness);
            lightness - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
        };
        let alpha = alpha(parts.get(3))?;
        return Ok(Paint::Fixed(Color::rgba(
            channel(0.0),
            channel(8.0),
            channel(4.0),
            alpha,
        )));
    }
    if let Some(rgb) = named_color(value) {
        return Ok(Paint::Fixed(Color::hex(rgb)));
    }
    let role = value
        .strip_prefix("theme(")
        .and_then(|v| v.strip_suffix(')'))
        .ok_or_else(|| StyleError::new(format!("unsupported color value: {value}")))?;
    let role = match role.trim() {
        "background" => Role::Background,
        "surface" => Role::Surface,
        "surface-alt" => Role::SurfaceAlt,
        "text" => Role::Text,
        "muted" => Role::Muted,
        "accent" => Role::Accent,
        "accent-hover" => Role::AccentHover,
        "accent-pressed" => Role::AccentPressed,
        "on-accent" => Role::OnAccent,
        "outline" => Role::Outline,
        "separator" => Role::Separator,
        "focus-ring" => Role::FocusRing,
        "selection" => Role::Selection,
        _ => return Err(StyleError::new(format!("unknown theme color: {role}"))),
    };
    Ok(Paint::Role(role))
}

/// The 16 basic CSS color keywords, plus `orange` and the `grey` spelling.
fn named_color(name: &str) -> Option<u32> {
    Some(match name {
        "black" => 0x000000,
        "silver" => 0xc0c0c0,
        "gray" | "grey" => 0x808080,
        "white" => 0xffffff,
        "maroon" => 0x800000,
        "red" => 0xff0000,
        "purple" => 0x800080,
        "fuchsia" => 0xff00ff,
        "green" => 0x008000,
        "lime" => 0x00ff00,
        "olive" => 0x808000,
        "yellow" => 0xffff00,
        "navy" => 0x000080,
        "blue" => 0x0000ff,
        "teal" => 0x008080,
        "aqua" => 0x00ffff,
        "orange" => 0xffa500,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_and_longhand_follow_source_order() {
        let patch = StylePatch::parse("padding: 4px 8px; padding-left: 20px;").unwrap();
        assert_eq!(
            patch.padding,
            [
                Some(Length::Px(4.0)),
                Some(Length::Px(8.0)),
                Some(Length::Px(4.0)),
                Some(Length::Px(20.0)),
            ]
        );
        let patch = StylePatch::parse("padding-left: 20px; padding: 4px;").unwrap();
        assert_eq!(patch.padding, [Some(Length::Px(4.0)); 4]);
    }

    #[test]
    fn cascade_resolves_each_property_by_specificity() {
        let sheet = StyleSheet::parse(
            r#"
            Container { width: 20px; background-color: #fff; }
            .card { width: 40px; }
            #main { width: 60px; }
            .card:hover { background-color: theme(accent); }
        "#,
        )
        .unwrap();
        let meta = StyleMeta::new().id("main").class("card");
        let base = sheet.resolve("Container", &meta, StyleState::default());
        assert_eq!(base.width, Some(Length::Px(60.0)));
        assert_eq!(base.background_color, Some(Paint::Fixed(Color::WHITE)));
        let hover = sheet.resolve(
            "Container",
            &meta,
            StyleState {
                hovered: true,
                ..Default::default()
            },
        );
        assert_eq!(hover.background_color, Some(Paint::Role(Role::Accent)));
    }

    #[test]
    fn unsupported_constructs_are_diagnostics() {
        assert!(StyleSheet::parse("Button[disabled] { color: #fff; }").is_err());
        assert!(StyleSheet::parse(".card + ~ Button { color: #fff; }").is_err());
        assert!(StyleSheet::parse("Button:nth-child(x) { color: #fff; }").is_err());
        assert!(StyleSheet::parse("Button:not(.a { color: #fff; }").is_err());
        assert!(StyleSheet::parse("@layer base { Button { color: #fff; } }").is_err());
        assert!(
            StyleSheet::parse("@media (orientation: portrait) { Button { color: #fff; } }")
                .is_err()
        );
        assert!(StyleSheet::parse(".card { opacity: 2; }").is_err());
        assert!(StyleSheet::parse(".card { color: notacolor; }").is_err());
        assert!(StyleSheet::parse(".card > > Button { color: #fff; }").is_err());
        assert!(StyleSheet::parse("> Button { color: #fff; }").is_err());
        assert!(StyleSheet::parse(".card { --gap: 4px; }").is_err());
        assert!(StyleSheet::parse(".card { gap: var(--missing); }").is_err());
        assert!(
            StyleSheet::parse(":root { --a: var(--b); --b: var(--a); } .c { gap: var(--a); }")
                .is_err()
        );
        assert!(StyleSheet::parse(".card { padding: -4px; }").is_err());
        assert!(StyleSheet::parse(".card { padding: auto; }").is_err());
        assert!(StyleSheet::parse(".card { color: rgb(300, 0, 0); }").is_err());
        assert!(StylePatch::parse("--gap: 4px;").is_err());
        assert!(StyleSheet::parse(".card { width: red; }").is_err());
        assert!(StyleSheet::parse("Button { transform: scale(2); }").is_err());
        assert!(StyleSheet::parse("Button { color: #foo; }").is_err());
        assert!(StyleSheet::parse("/* not closed").is_err());
    }

    #[test]
    fn malformed_nth_and_color_arguments_are_errors() {
        for bad in [
            "Button:nth-child(2n3) { color: #fff; }",
            "Button:nth-last-child(n2) { color: #fff; }",
            ".a { color: rgb(1,,2,3); }",
            ".a { color: rgb(1, 2, 3,); }",
            ".a { color: rgb(1 2, 3); }",
            ".a { color: rgb(1, 2 3); }",
            ".a { color: rgb(1 2 3 4); }",
            ".a { color: rgb(1 2 / 3); }",
            ".a { color: rgb(1 2 3 / ); }",
            ".a { color: rgb(1, 2, 3 / 0.5); }",
            ".a { color: hsl(0 100% 50% / 1 / 1); }",
        ] {
            assert!(StyleSheet::parse(bad).is_err(), "{bad}");
        }
        for good in [
            "Button:nth-child(2n+3) { color: #fff; }",
            "Button:nth-child( 2n - 1 ) { color: #fff; }",
            "Button:nth-child(-n+3) { color: #fff; }",
            "Button:nth-child(n) { color: #fff; }",
            "Button:nth-child(+5) { color: #fff; }",
            ".a { color: rgb( 1 , 2 , 3 ); }",
            ".a { color: rgba(1,2,3,50%); }",
            ".a { color: rgb(1 2 3/0.5); }",
            ".a { color: hsla(120deg 100% 50% / 50%); }",
        ] {
            assert!(StyleSheet::parse(good).is_ok(), "{good}");
        }
    }

    fn node<'a>(widget_type: &'a str, meta: &'a StyleMeta) -> StyleNode<'a> {
        StyleNode {
            widget_type,
            meta,
            state: StyleState::default(),
            index: 0,
            sibling_count: 1,
        }
    }

    struct Family<'a> {
        parent: StyleNode<'a>,
        children: Vec<StyleNode<'a>>,
    }

    impl StyleTree for Family<'_> {
        /// `None` is the parent.
        type Id = Option<usize>;

        fn style_node(&self, id: Self::Id) -> StyleNode<'_> {
            match id {
                Some(index) => StyleNode {
                    index,
                    sibling_count: self.children.len(),
                    ..self.children[index]
                },
                None => self.parent,
            }
        }

        fn style_parent(&self, id: Self::Id) -> Option<Self::Id> {
            id.map(|_| None)
        }

        fn style_previous_sibling(&self, id: Self::Id) -> Option<Self::Id> {
            id?.checked_sub(1).map(Some)
        }
    }

    #[test]
    fn sibling_combinators_and_positions_match_within_a_parent() {
        let sheet = StyleSheet::parse(
            r#"
            Label:first-child { width: 1px; }
            Label:last-child { height: 2px; }
            Label:nth-child(2n + 1) { min-width: 3px; }
            Label:nth-last-child(2) { min-height: 4px; }
            .lead + Label { max-width: 5px; }
            .lead ~ Label { max-height: 6px; }
            Label:not(.lead):not(:last-child) { opacity: 0.5; }
            Label:only-child { display: none; }
            "#,
        )
        .unwrap();
        assert!(sheet.needs_siblings());
        let (plain, lead) = (StyleMeta::new(), StyleMeta::new().class("lead"));
        let family = Family {
            parent: node("Container", &plain),
            children: vec![
                node("Label", &lead),
                node("Label", &plain),
                node("Label", &plain),
            ],
        };
        let child = |index| sheet.resolve_in(&family, Some(index), &StylePatch::EMPTY);
        let (first, second, third) = (child(0), child(1), child(2));
        assert_eq!(first.width, Some(Length::Px(1.0)));
        assert_eq!(second.width, None);
        assert_eq!(third.height, Some(Length::Px(2.0)));
        assert_eq!(first.height, None);
        assert_eq!(
            [first.min_width, second.min_width, third.min_width],
            [Some(Length::Px(3.0)), None, Some(Length::Px(3.0))]
        );
        assert_eq!(second.min_height, Some(Length::Px(4.0)));
        assert_eq!(third.min_height, None);
        assert_eq!(
            [first.max_width, second.max_width, third.max_width],
            [None, Some(Length::Px(5.0)), None]
        );
        assert_eq!(
            [first.max_height, second.max_height, third.max_height],
            [None, Some(Length::Px(6.0)), Some(Length::Px(6.0))]
        );
        assert_eq!(
            [first.opacity, second.opacity, third.opacity],
            [None, Some(0.5), None]
        );
        assert_eq!(first.display, None);
        let alone = sheet.resolve("Label", &plain, StyleState::default());
        assert!(alone.is_display_none());
    }

    #[test]
    fn media_rules_follow_the_viewport() {
        let mut sheet = StyleSheet::parse(
            r#"
            .side { width: 200px; }
            @media (max-width: 600px) and (min-height: 100px) {
                .side { width: 80px; }
                .side:hover { width: 90px; }
            }
            "#,
        )
        .unwrap();
        let meta = StyleMeta::new().class("side");
        let width = |sheet: &StyleSheet| {
            sheet
                .resolve("Container", &meta, StyleState::default())
                .width
        };
        assert_eq!(width(&sheet), Some(Length::Px(200.0)));
        assert!(sheet.set_viewport(500.0, 300.0));
        assert_eq!(width(&sheet), Some(Length::Px(80.0)));
        assert!(!sheet.set_viewport(550.0, 300.0));
        assert!(sheet.set_viewport(500.0, 50.0));
        assert_eq!(width(&sheet), Some(Length::Px(200.0)));
    }

    #[test]
    fn hsl_and_named_colors() {
        let color = |source: &str| StylePatch::parse(source).unwrap().color;
        let fixed = |color: Color| Some(Paint::Fixed(color));
        assert_eq!(color("color: red;"), fixed(Color::hex(0xff0000)));
        assert_eq!(
            color("color: hsl(0, 100%, 50%);"),
            fixed(Color::hex(0xff0000))
        );
        assert_eq!(
            color("color: hsla(120deg 100% 50% / 50%);"),
            fixed(Color::hex(0x00ff00).with_alpha(0.5))
        );
        assert_eq!(color("color: hsl(240, 100%, 100%);"), fixed(Color::WHITE));
    }

    #[test]
    fn combinators_match_along_the_ancestor_path() {
        let sheet = StyleSheet::parse(
            r#"
            .panel Label { width: 10px; }
            .panel > Label { height: 20px; }
            .app .panel>Label { min-width: 30px; }
        "#,
        )
        .unwrap();
        assert!(sheet.needs_ancestors());
        let (plain, panel, app) = (
            StyleMeta::new(),
            StyleMeta::new().class("panel"),
            StyleMeta::new().class("app"),
        );
        let child = sheet.resolve_path(
            &[
                node("Label", &plain),
                node("Container", &panel),
                node("Container", &app),
            ],
            &StylePatch::EMPTY,
        );
        assert_eq!(child.width, Some(Length::Px(10.0)));
        assert_eq!(child.height, Some(Length::Px(20.0)));
        assert_eq!(child.min_width, Some(Length::Px(30.0)));
        let grandchild = sheet.resolve_path(
            &[
                node("Label", &plain),
                node("Container", &plain),
                node("Container", &panel),
            ],
            &StylePatch::EMPTY,
        );
        assert_eq!(grandchild.width, Some(Length::Px(10.0)));
        assert_eq!(grandchild.height, None);
        assert_eq!(grandchild.min_width, None);
        assert!(
            sheet
                .resolve("Label", &plain, StyleState::default())
                .is_empty()
        );
    }

    #[test]
    fn variables_resolve_globally_with_fallbacks() {
        let sheet = StyleSheet::parse(
            r#"
            .card { gap: var(--gap); color: var(--ink, #fff); border: 1px solid var(--edge); }
            :root { --gap: var(--unit); --unit: 6px; --edge: rgb(0, 0, 0); }
        "#,
        )
        .unwrap();
        let card = sheet.resolve(
            "Container",
            &StyleMeta::new().class("card"),
            StyleState::default(),
        );
        assert_eq!(card.row_gap, Some(6.0));
        assert_eq!(card.color, Some(Paint::Fixed(Color::WHITE)));
        assert_eq!(card.border_color, Some(Paint::Fixed(Color::BLACK)));
        let inline = sheet.parse_inline("padding: var(--unit);").unwrap();
        assert_eq!(inline.padding, [Some(Length::Px(6.0)); 4]);
        assert!(StylePatch::parse("padding: var(--unit);").is_err());
    }

    #[test]
    fn variables_expand_once_and_report_broken_ones_even_when_unused() {
        // Each level uses the one before ten times: 10^8 copies if expanded naively.
        let mut css = String::from(":root { --v0: 1px;");
        for level in 1..=8 {
            let uses = vec![format!("var(--v{})", level - 1); 10].join(" ");
            css.push_str(&format!(" --v{level}: {uses};"));
        }
        css.push_str(" } .card { gap: var(--v8); }");
        let started = std::time::Instant::now();
        let error = StyleSheet::parse(&css).unwrap_err();
        assert!(error.to_string().contains("more than"), "{error}");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));

        let mut css = String::from(":root { --c0: 7px;");
        for i in 1..40 {
            css.push_str(&format!(" --c{i}: var(--c{});", i - 1));
        }
        css.push_str(" } .card { gap: var(--c39); }");
        let sheet = StyleSheet::parse(&css).unwrap();
        let card = sheet.resolve(
            "Container",
            &StyleMeta::new().class("card"),
            StyleState::default(),
        );
        assert_eq!(card.row_gap, Some(7.0));

        let cycle = StyleSheet::parse(":root { --a: var(--b); --b: var(--a); }").unwrap_err();
        assert!(cycle.to_string().contains("--a -> --b -> --a"), "{cycle}");
        assert!(StyleSheet::parse(":root { --x: var(--missing); }").is_err());
        assert!(StyleSheet::parse(":root { --x: var(--missing, 2px); }").is_ok());
    }

    #[test]
    fn deeply_nested_fallbacks_are_an_error_not_a_stack_overflow() {
        let mut value = String::from("1px");
        for _ in 0..3000 {
            value = format!("var(--missing, {value})");
        }
        let error = StyleSheet::parse(&format!(".card {{ gap: {value}; }}")).unwrap_err();
        assert!(error.to_string().contains("nest"), "{error}");
        let sheet = StyleSheet::parse(".card { gap: var(--a, var(--b, 3px)); }").unwrap();
        let card = sheet.resolve(
            "Container",
            &StyleMeta::new().class("card"),
            StyleState::default(),
        );
        assert_eq!(card.row_gap, Some(3.0));
    }

    #[test]
    fn important_on_a_variable_stays_with_the_variable() {
        let sheet =
            StyleSheet::parse(":root { --ink: #123456 !important; } .card { color: var(--ink); }")
                .unwrap();
        let card = sheet.resolve(
            "Container",
            &StyleMeta::new().class("card"),
            StyleState::default(),
        );
        assert_eq!(card.color, Some(Paint::Fixed(Color::hex(0x123456))));
        let inline = StylePatch::parse("color: #ffffff;").unwrap();
        let resolved = sheet.resolve_path(
            &[node("Container", &StyleMeta::new().class("card"))],
            &inline,
        );
        assert_eq!(resolved.color, Some(Paint::Fixed(Color::WHITE)));
    }

    #[test]
    fn rules_apply_by_specificity_then_source_order() {
        let mut sheet = StyleSheet::parse(
            r#"
            #main { width: 50px; }
            .card { width: 10px; height: 10px; opacity: 0.1 !important; }
            Container { width: 1px; height: 1px; opacity: 0.2 !important; }
            .card { height: 20px; }
            @media (min-width: 500px) { .card { height: 30px; } }
            .wide { opacity: 0.3 !important; }
            Container { opacity: 0.4 !important; }
            #main { width: 60px; }
        "#,
        )
        .unwrap();
        let meta = StyleMeta::new().id("main").class("card").class("wide");
        let resolved =
            |sheet: &StyleSheet| sheet.resolve("Container", &meta, StyleState::default());
        let narrow = resolved(&sheet);
        // The later of two equally specific rules wins, a more specific one wherever
        // it is; `!important` declarations compare the same way among themselves.
        assert_eq!(narrow.width, Some(Length::Px(60.0)));
        assert_eq!(narrow.height, Some(Length::Px(20.0)));
        assert_eq!(narrow.opacity, Some(0.3));
        sheet.set_viewport(800.0, 600.0);
        assert_eq!(resolved(&sheet).height, Some(Length::Px(30.0)));
        let plain = sheet.resolve("Container", &StyleMeta::new(), StyleState::default());
        assert_eq!(plain.width, Some(Length::Px(1.0)));
        assert_eq!(plain.opacity, Some(0.4));
    }

    #[test]
    fn important_outranks_specificity_and_inline() {
        let sheet = StyleSheet::parse(
            r#"
            Container { width: 10px !important; height: 10px; }
            #main { width: 20px; height: 20px; }
        "#,
        )
        .unwrap();
        let meta = StyleMeta::new().id("main");
        let inline = StylePatch::parse("width: 30px; height: 30px;").unwrap();
        let resolved = sheet.resolve_path(&[node("Container", &meta)], &inline);
        assert_eq!(resolved.width, Some(Length::Px(10.0)));
        assert_eq!(resolved.height, Some(Length::Px(30.0)));
        let patch = StylePatch::parse("width: 1px !important; width: 2px;").unwrap();
        assert_eq!(patch.width, Some(Length::Px(1.0)));
    }

    #[test]
    fn values_cover_auto_negative_offsets_and_color_forms() {
        let patch = StylePatch::parse(
            "width: auto; margin: -4px auto; max-width: none; top: -10%; flex: 2; \
             align-items: center; justify-content: space-between; font-weight: bold; \
             text-align: right; position: absolute; flex-wrap: wrap;",
        )
        .unwrap();
        assert_eq!(patch.width, Some(Length::Auto));
        assert_eq!(patch.margin[0], Some(Length::Px(-4.0)));
        assert_eq!(patch.margin[1], Some(Length::Auto));
        assert_eq!(patch.max_width, Some(Length::Auto));
        assert_eq!(patch.inset[0], Some(Length::Percent(-0.1)));
        assert_eq!(
            (patch.flex_grow, patch.flex_shrink, patch.flex_basis),
            (Some(2.0), Some(1.0), Some(Length::Px(0.0)))
        );
        assert_eq!(patch.align_items, Some(AlignItems::CENTER));
        assert_eq!(patch.justify_content, Some(JustifyContent::SPACE_BETWEEN));
        assert_eq!(patch.font_weight, Some(700));
        assert_eq!(patch.text_align, Some(TextAlign::End));
        assert_eq!(patch.position, Some(Position::Absolute));
        assert_eq!(patch.flex_wrap, Some(FlexWrap::Wrap));

        let color = |source: &str| StylePatch::parse(source).unwrap().color;
        let half = Paint::Fixed(Color::hex(0xff0000).with_alpha(0.5));
        assert_eq!(color("color: rgba(255, 0, 0, 0.5);"), Some(half));
        assert_eq!(color("color: rgb(255 0 0 / 50%);"), Some(half));
        assert_eq!(
            color("color: #ff000080;"),
            Some(Paint::Fixed(Color::hex(0xff0000).with_alpha(128.0 / 255.0)))
        );
        assert_eq!(
            color("color: #f00f;"),
            Some(Paint::Fixed(Color::hex(0xff0000)))
        );
    }

    #[test]
    fn nth_child_with_extreme_coefficients_does_not_overflow() {
        let sheet = StyleSheet::parse(
            r#"
            Label:nth-child(-n-2147483647) { width: 1px; }
            Label:nth-child(-1n+2147483647) { height: 2px; }
            Label:nth-last-child(2147483647n-2147483647) { min-width: 3px; }
            "#,
        )
        .unwrap();
        let plain = StyleMeta::new();
        let resolved = sheet.resolve("Label", &plain, StyleState::default());
        assert_eq!(resolved.width, None);
        assert_eq!(resolved.height, Some(Length::Px(2.0)));
        assert_eq!(resolved.min_width, None);
    }

    #[test]
    fn only_child_counts_as_one_pseudo_class() {
        let sheet =
            StyleSheet::parse("Label:only-child { color: red; } Label.selected { color: blue; }")
                .unwrap();
        let meta = StyleMeta::new().class("selected");
        let resolved = sheet.resolve("Label", &meta, StyleState::default());
        assert_eq!(resolved.color, Some(Paint::Fixed(Color::hex(0x0000ff))));
    }

    #[test]
    fn repeated_media_bounds_must_all_hold() {
        let mut sheet = StyleSheet::parse(
            r#"
            @media (min-width: 800px) and (min-width: 400px) { .a { width: 1px; } }
            @media (max-width: 400px) and (max-width: 800px) { .a { height: 2px; } }
            @media (min-height: 800px) and (min-height: 400px) { .a { min-width: 3px; } }
            @media (max-height: 400px) and (max-height: 800px) { .a { min-height: 4px; } }
            "#,
        )
        .unwrap();
        sheet.set_viewport(600.0, 600.0);
        let meta = StyleMeta::new().class("a");
        assert!(
            sheet
                .resolve("Label", &meta, StyleState::default())
                .is_empty()
        );
        sheet.set_viewport(900.0, 300.0);
        let resolved = sheet.resolve("Label", &meta, StyleState::default());
        assert_eq!(resolved.width, Some(Length::Px(1.0)));
        assert_eq!(resolved.min_height, Some(Length::Px(4.0)));
    }

    #[test]
    fn inline_custom_properties_are_always_errors() {
        let sheet = StyleSheet::parse(":root { --gap: 4px; }").unwrap();
        assert!(
            sheet
                .parse_inline("--gap: 20px; padding: var(--gap);")
                .is_err()
        );
        assert!(sheet.parse_inline("--other: 20px;").is_err());
        assert_eq!(
            sheet.parse_inline("padding: var(--gap);").unwrap().padding,
            [Some(Length::Px(4.0)); 4]
        );
    }
}
