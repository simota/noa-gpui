# Design: multiple windows inside a window (desktop-like container)

A container that lays out child windows inside an OS window, behaving like OS windows. Status: implemented ([#16](https://github.com/simota/noa-gpui/issues/16)). Decisions made during implementation, and the points changed from this design, are in the appendix of `docs/architecture.md`.

## Decisions

- **Child windows are Widgets in the tree (not overlays).** `InnerWindow`s are laid out as children of the container `Desktop`, and the order of children is the stacking order as is (later children are in front).
- **Window management is the job of the controller `WindowManager`.** Same shape as `ColorPanel` / `FontPanel` (`crates/ui-core/src/widgets/standard_panels.rs`): the app owns it and passes messages to `handle(&mut UiTree, &Message)`. A Widget can only touch itself (`EventCx` only makes requests about itself). So cross-cutting work such as snapping by looking at siblings' positions, changing the stacking order, and hiding minimized windows goes in the controller, which can touch the whole tree.
- **`InnerWindow` draws its own window frame (title bar, edges, buttons).** Drawing frames and contents in stacking order requires drawing per window (if `Desktop` drew them all at once, a lower window's contents would cover an upper window's frame).

## Options considered and rejected

| Option | Why rejected |
|---|---|
| Extend #14's `Panel` (overlay) | Overlays are placed relative to the whole OS window (`place` in `crates/ui-core/src/overlay.rs`). They cannot be confined to the container's area, and there is no clipping and no per-container stacking order. `Panel` stays as is, as a panel that floats above everything |
| A single `Desktop` Widget owns everything (and draws the frames) | It has the stacking-order problem above, and moving a child requires changing the child's style (there is no way to rewrite a child from outside) |
| Widgets notify each other (add a new message to the parent) | Snapping needs the positions of all siblings. A place that sees the whole tree is needed in the end, and only the notification mechanism is added |

## Parts and API (shape only)

```rust
// ui-core/src/widgets/desktop.rs
pub struct Desktop;            // Draws the background, the dock (strip along the bottom edge), and the snap target preview. Children are InnerWindows only
pub struct InnerWindow;        // Title, frame, close / minimize / maximize buttons. Children are the contents
pub struct WindowManager { desktop: WidgetId, windows: Vec<WindowState>, active: Option<WidgetId> }

impl WindowManager {
    pub fn new(tree: &mut UiTree, parent: WidgetId, style: Style) -> Self;
    pub fn open(&mut self, tree: &mut UiTree, title: &str, frame: Option<Rect>) -> WidgetId; // Returns the parent of the contents
    pub fn close / minimize / restore / maximize / activate(&mut self, tree, window);
    pub fn present_sheet(&mut self, tree, window, sheet: Sheet) -> WidgetId;   // In-window modal
    pub fn present_alert(&mut self, tree, window, alert: Alert) -> WidgetId;
    pub fn handle(&mut self, tree: &mut UiTree, message: &Message) -> Option<WindowEvent>;
}
pub enum WindowEvent { Activated(WidgetId), Closed(WidgetId), TornOff { window: WidgetId, at: Point } }
```

- `InnerWindow` emits interactions as `WindowGesture` (title bar drag amount, edge and corner drag amount, buttons, being pressed). `InnerWindow` holds its position and size and reflects them in the style's absolute positioning (`position: absolute` and `inset`). The controller decides the position with `tree.update::<InnerWindow>` (snapping, confinement to the area, minimum size).
- Stacking order moves the child to the end with `UiTree::move_child` (`crates/ui-core/src/tree.rs:581`). Minimizing uses `UiTree::set_hidden`; `Desktop` draws the dock items and takes the pointer with `Widget::claims_pointer`. The snap preview is drawn above the children with `Widget::paint_over`.

## Additions to the tree (`UiTree`)

1. **Focus scope (`Widget::focus_scope() -> bool`):** while focus is inside it, Tab cycles only within it (currently, only modal overlays can narrow the range: `focus_next`).
2. **Press notification (`Event::PressWithin`, with `Widget::observes_presses() -> bool` narrowing which Widgets receive it):** even if a button inside handles the press and stops it, the window learns it was pressed and can become active. A notification-only event that cannot be marked handled.
3. **Modal within a scope (`Widget::modal() -> bool`):** a modal Widget blocks input to the rest of the nearest focus scope (the window). It applies to hit testing, the key target (`key_target`), and the Tab range. Reuse #14's `Sheet` and `Alert` by adding a "show inside the window" setting, and extend `EventCx::close_overlay` so that outside overlays it removes that Widget.
4. **Stop relayouting everything on reorder:** `move_child` currently sets `structure_changed` and relayouts the whole tree. Reordering absolutely positioned children does not change layout, so compositing alone is enough.
5. **Taking out and inserting subtrees (`UiTree::take_subtree` / `insert_subtree`):** move to another tree (another OS window) while keeping Widget state. Used for dragging between windows.

## Behavior

- Move: drag the title bar. Constrained so that the title bar always stays inside the container. Double-click toggles between maximized and restored.
- Resize: 4 edges and 4 corners (grab width 6px). Respects the minimum size. The pointer shape is not changed this time because there is no Cursor API.
- Activation: bring to front with a click or the window-switching key. Inactive windows draw their title bar faded.
- Maximize: the whole container excluding the dock. Remembers the original position and size.
- Minimize: hide the window and add an item (the title) to the dock. Clicking the item restores the window and activates it.
- Snapping: if a moving edge is within 8px of an edge of the container or of another window, align them. When the pointer reaches the left or right end, left half / right half; when it reaches the top end, maximize. Draw a preview until release.
- Keys: ⌘` / Ctrl+F6 for the next window, ⌘W to close, ⌘M to minimize (`Widget::key_equivalents`). Tab cycles only within the window. Ctrl+Tab (reversed with Shift) exits to the next focus target outside the window (so the keyboard is not trapped in the window; WCAG 2.1.2).
- In-window modal: the sheet slides down from below the title bar. It blocks only that window; other windows stay operable.
- Dragging between windows: pulling the title bar out beyond the container returns `WindowEvent::TornOff` to the app. The desktop demo opens an OS window with the taken-out subtree. Returning it is done with "Return to desktop" in that OS window's menu. Returning by dragging the OS window over the container needs tracking of another OS window's position, so decide after actually trying it at the last stage.
- Accessibility: `InnerWindow` is `Role::Window` (label is the title, `modal` while modal). Buttons and dock items can be operated by clicks from assistive technology.

## Build order

1. Tree additions 1, 2, and 4, and moving, resizing, stacking order, activation, closing, keys, and accessibility for `InnerWindow` / `Desktop` / `WindowManager`. Add to the catalog. **Do the measurement below first.**
2. Minimize and the dock, maximize.
3. Snapping and the preview.
4. In-window modals (tree addition 3).
5. Dragging between windows (tree addition 5 and the demo's OS windows).

## Conditions under which this design breaks

**If layout on every frame during a drag is heavy.** Moving is a change to `inset`, so the placement of the container's children is recomputed. Each window's contents do not change size, so the expectation is that Taffy's cache keeps them from being recomputed.

- How to check: add a benchmark to `crates/ui-core/benches/` that places 20 windows with 500 Widgets of content each and moves one every frame, and look at the p95 of `prepare`.
- Alternative if it breaks: switch to holding positions outside layout (a new hook that offsets each child at compositing). In that case, there is one more tree addition.

## Not included

Window open/close animations (genie effect, etc.), saving and restoring placement, windows grouped into tabs, virtual desktops, tiling finer than halves, touch input, changing the pointer shape, dragging between multiple `Desktop`s.
