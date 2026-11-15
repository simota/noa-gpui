# Noa Style (CSS-like styling)

Noa Style overrides layout and painting with CSS-like declarations while keeping the existing `Widget` / `UiTree`.

> It is a **subset** of CSS. It is not compatible with browser CSS. Unknown properties and selector syntax return a `StyleError` instead of being silently ignored. Type selector names (such as `Button`) are not checked, however. This is because custom Widget type names can also be used; a misspelled type name (`Buton`) matches nothing.

## Usage

```rust
use noa_gpui_core::styling::{StyleMeta, StylePatch, StyleSheet};
use noa_gpui_core::widgets::{Button, Container, Label};
use noa_gpui_core::UiTree;

let mut tree = UiTree::new();
let root = tree.insert(None, Container::column());
tree.set_style_meta(root, StyleMeta::new().class("settings"));

let title = tree.insert(Some(root), Label::new("Settings"));
tree.set_style_meta(title, StyleMeta::new().id("title"));

let save = tree.insert(Some(root), Button::new("Save"));
tree.set_style_meta(save, StyleMeta::new().class("primary"));

tree.set_stylesheet(StyleSheet::parse(r#"
    .settings {
        padding: 24px;
        gap: 12px;
        background-color: theme(background);
    }

    #title {
        color: theme(text);
        font-size: 22px;
    }

    Button.primary {
        background-color: theme(accent);
        color: theme(on-accent);
        border-radius: 8px;
    }

    Button.primary:hover {
        background-color: theme(accent-hover);
    }
"#)?);

// Dynamic one-off values outrank matching stylesheet declarations.
tree.set_inline_style(save, StylePatch::parse("min-width: 120px;")?);
```

`StyleSheet::parse` and `StylePatch::parse` return a `Result`. They work on strings read from a file at runtime as well as strings embedded with `include_str!`.

```rust
let css = include_str!("app.noa.css");
tree.set_stylesheet(StyleSheet::parse(css)?);
```

## Currently supported

- **Selectors:** Widget types such as `Container` / `Button`, `*`, `#id`, `.class`, compound selectors (`Button.primary`), comma-separated lists
- **Combinators:** descendant (whitespace, `.panel Label`), child (`>`), next sibling (`+`), subsequent sibling (`~`)
- **State:** `:hover`, `:focus`, `:focus-within`, `:focus-visible` (focus moved by something other than a pointer press), `:active` (while the pointer is pressed; the pressed node and its ancestors), `:disabled`, `:root` (a node with no parent)
- **Position:** `:first-child`, `:last-child`, `:only-child`, `:nth-child()`, `:nth-last-child()` (`odd`, `even`, `3`, `2n+1`; after `n` write nothing or a sign; `2n3` is an error)
- **Negation:** `:not()` (one compound selector inside; `Label:not(.lead):not(:last-child)`)
- **`@media`:** `min-width`, `max-width`, `min-height`, `max-height` (px, joined with `and`). Switches with the window size
- **Size:** `width`, `height`, `min-width`, `min-height`, `max-width`, `max-height` (non-negative px / %; `auto`, and `none` for `max-*`)
- **Spacing:** `padding` (non-negative px / %), `margin` (px / %, negative values and `auto` allowed), per-side longhands, 1–4 value shorthands
- **Display:** `display` (`flex` / `block` / `none`), `overflow` (`visible` / `hidden` / `clip`), `opacity` (0–1 or %; fades the node together with its descendants)
- **Positioning:** `position` (`relative` / `absolute`), `top` / `right` / `bottom` / `left`, the `inset` shorthand (px / %, negative values and `auto` allowed)
- **Flex:** `flex-direction`, `flex-wrap`, `flex-grow`, `flex-shrink`, `flex-basis`, the `flex` shorthand (`none`, `<grow> [<shrink> [<basis>]]`), `align-items`, `align-self`, `justify-content`, `gap`, `row-gap`, `column-gap` (gaps are non-negative px)
- **Painting:** `background-color`, `color`, `border`, `border-color`, `border-width`, `border-radius`
- **Text:** `font-size` (positive px), `font-weight` (`normal`, `bold`, 1–1000), `text-align` (`start` / `left`, `center`, `end` / `right`)
- **Colors:** `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()` / `rgba()`, `hsl()` / `hsla()` (either all comma-separated, `rgb(0, 128, 255, 0.5)`, or space-separated with alpha after `/`, `rgb(0 128 255 / 50%)`; mixed syntax and empty arguments are errors; alpha is 0–1 or %), basic color names (17 colors such as `red`, `white`, `gray`), `transparent`, `theme(role)`. Maps to the existing `Role` (e.g. `accent`, `surface-alt`, `focus-ring`)
- **Variables:** declare with `--name: value;` inside `:root`, and use with `var(--name)` / `var(--name, fallback)`
- **`!important`:** takes precedence over specificity and inline styles

## Applying to Widgets

| Property | Widgets it affects |
|---|---|
| Layout (size, spacing, Flex, positioning, `display`, `overflow`, `opacity`) | All (applied to the node's Taffy style and at compositing) |
| `background-color`, `border`, `border-radius` | All. `Container`, `Button`, and text inputs (below) build it into their own shape; for everything else the tree draws a box **under** the Widget's painting (including custom Widgets) |
| `color`, `font-size`, `font-weight` | `Label`, `Button`, text inputs |
| `text-align` | `Label` |

The text inputs are `TextField`, `SecureTextField`, `SearchField`, `ComboBox`, `TextArea`, and `TextView`. `background-color`, `border`, and `border-radius` replace the fill, stroke, and corners of the standard frame (bezel). For `TokenField`, `NumberInput`, and `ShortcutRecorder`, only the frame changes.

- The focused frame keeps the accent color (at least 2px wide) unless `border-color` is specified. To change the focused color, write something like `TextField:focus { border-color: ... }`
- `color` applies to the entered text; the placeholder stays a faint color
- Like `Button`, text inputs do not inherit from ancestors (as with the browser's `input`). Specify them directly
- A single-line input whose width is changed with `width` or similar scrolls and clips its text within the new width

```css
TextField, TextArea {
    background-color: #fffbea;
    border: 1px solid #d4a72c;
    border-radius: 4px;
    color: #5c3c00;
}
TextField:focus { border-color: theme(accent); }

/* For Widgets without their own box, the tree draws the box */
Checkbox:hover { background-color: theme(surface-alt); border-radius: 4px; }
```

When a custom Widget builds the box into its own shape, it returns `true` from `Widget::paints_style_box` and uses the values from `PaintCx::styling` itself.

The layout width of `border` is applied when specified in CSS. The existing `Container::border` stays as it was for compatibility. Text placement inside buttons and decoration from the native theme also follow the existing implementation.

A `position: absolute` node is placed relative to its parent regardless of the parent's `position` (a rule of `noa-gpui-layout`).

A `display: none` node is treated the same as `UiTree::set_hidden`. It takes no space, is not drawn, and receives neither focus nor hover.

## Loading from a file and reloading on save (desktop)

`App::watch_stylesheet` applies a style sheet from a file to the main screen and reapplies it every time the file is saved while the app keeps running. A file that cannot be read or parsed is logged, and the previous style sheet stays in use. Fix it and save, and it is loaded again.

```rust
App::new(options, build)
    .watch_stylesheet("app.noa.css")
    .run()?;
```

Watching runs on a separate thread and checks the file's modification time and length every 250ms (no additional dependencies). A changed file is read once it has stayed unchanged until the next check (so that it does not read an empty file while an editor is emptying and rewriting it). If the contents are the same, it is not reloaded. For screens other than the main one, or to decide how to apply it yourself, use `StyleSheetWatcher` directly and `take` in `Screen::woken`.

```rust
let watcher = StyleSheetWatcher::new("panel.noa.css", setup.waker.clone());
// Inside Screen::woken
if let Some(Ok(sheet)) = self.watcher.take() {
    self.tree.set_stylesheet(sheet);
}
```

The demo lets you try it with `--stylesheet` (`apps/demo/styles/example.noa.css`). Combined with `--snapshot`, it writes the styled screen to a PNG.

```sh
cargo run -p desktop --release -- --scene=catalog --stylesheet=apps/demo/styles/example.noa.css
```

The web has no file watching. Pass a string read with `include_str!` or similar to `StyleSheet::parse`.

## Inheritance

`color`, `font-size`, `font-weight`, and `text-align` pass down to descendants that do not specify them. `computed_style` includes inherited values.

```css
.sidebar { color: theme(muted); font-size: 12px; }   /* applies to every Label inside */
```

`Button` does not inherit (like browser buttons, because it has its own text color). Specify buttons directly. Custom Widgets choose with `Widget::inherits_style`.

## @media

```css
.sidebar { width: 240px; }

@media (max-width: 600px) {
    .sidebar { display: none; }
}
```

Conditions are evaluated against the viewport passed to `UiTree::prepare`, and styles are re-resolved only when a size change flips a condition. Before the first `prepare`, rules inside `@media` have no effect.

## Combinators

```css
.sidebar Label { color: theme(muted); }          /* every Label inside .sidebar */
.toolbar > Button { border-radius: 4px; }        /* Buttons that are direct children of .toolbar */
.card:hover > Label { color: theme(accent); }    /* change descendants by an ancestor's state */
```

```css
.field:focus-within + Label { color: theme(accent); }   /* next sibling */
.row:nth-child(even) { background-color: theme(surface-alt); }
.row:not(:last-child) { margin-bottom: 4px; }
```

When the state, ID, or class of an ancestor or sibling changes, the nodes that could match through it are re-resolved. For a style sheet with no rules that use combinators, this traversal is skipped. For a style sheet with rules that look at siblings or position (`+`, `~`, `:nth-child`, and so on), styles are not resolved on every child insertion, removal, or reorder; instead the parent's children are resolved together at the next `prepare`. The `computed_style` of a just-inserted node is also determined then.

## Variables

```css
:root {
    --gap: 12px;
    --edge: rgba(0, 0, 0, 0.2);
}

.settings { gap: var(--gap); border: 1px solid var(--edge); }
.settings > Label { margin-left: var(--indent, 0); }
```

Variables form a single set for the whole style sheet. Declaring them in a rule other than `:root` is an error, and there is no per-node overriding or inheritance. Values are expanded at `StyleSheet::parse` time, so undefined variables (with no fallback) and circular references are parse errors. Declaration position does not matter; a variable defined in a later `:root` can be used by earlier rules. Variables referencing variables, and nesting of `var()` fallbacks, are limited to 64 levels, and a value after expanding `var()` is limited to 16 KiB; exceeding either is a parse error (so that expansion cannot grow without bound, even for style sheets loaded at runtime). A trailing `!important` on a variable's value belongs to that variable's declaration and does not make the declaration that uses it via `var()` `!important`.

To use a style sheet's variables in an inline style, parse it with that style sheet.

```rust
let sheet = StyleSheet::parse(css)?;
tree.set_inline_style(save, sheet.parse_inline("min-width: var(--button-width);")?);
tree.set_stylesheet(sheet);
```

## Precedence

From lowest to highest:

0. Values inherited from ancestors (the four such as `color`)
1. Values set by a Widget's builder or `set_*` (`Label::font_size`, `Label::color`, and so on). Inherited values do not override these. `computed_style` includes inherited values too, and `StylePatch::is_inherited` tells which is which
2. Style sheet rules. ID > class and pseudo-class > type; with equal specificity, the later rule wins. For selectors joined by combinators, the specificities of each part are added
3. Properties specified with `set_inline_style`
4. Declarations with `!important` (in the same order as 2 among themselves)

Declarations override per property. Shorthands such as `padding` are expanded into each side on the spot. `!important` inside an inline style only takes precedence over the other declarations in the same inline style.

Clearing both the style sheet and the inline style returns to the settings from the Widget-specific builder.

```rust
tree.set_inline_style(save, StylePatch::default());
tree.set_stylesheet(StyleSheet::default());
```

Style metadata is held on the tree's nodes, so existing typed update APIs such as `UiTree::update::<Button>` keep working as before. On a state change, the target node and the necessary ancestors and descendants are re-resolved, and only changes to layout-related values request `Dirty::LAYOUT`. Express custom states by adding and removing classes with `set_style_meta`.

## Out of scope

Noa Style is not a compatible implementation of browser CSS; it is a DSL that handles, with types, the parts that are meaningful for this UI tree. The following are out of scope by design, and writing them produces a `StyleError` (they are not silently ignored).

- **Attribute selectors, pseudo-elements such as `::part`:** nodes have neither attributes nor parts. Use classes to distinguish them
- **Grid, `transform`:** layout is Flex, block, and absolute positioning. Motion is handled by Widget-side animation
- **`calc()`, units other than px and %:** lengths are only logical pixels and percentages of the parent
- **Per-node variables, `@layer`, nested rules, the `inherit` keyword:** variables are a single global set, and precedence has only the three levels in the "Precedence" section

## Verification

```sh
cargo test -p noa-gpui-core --test styling
cargo test -p noa-gpui-platform --features native --lib stylesheet
./scripts/verify.sh
```

New behavior is verified in `crates/ui-core/tests/styling.rs` and the unit tests in `styling.rs`. When changing an implementation constraint, update the tests and this table at the same time.
