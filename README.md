# noa-gpui

A cross-platform UI framework written in Rust, with its own 2D renderer on top of wgpu.

> **Status:** Phases 1, 2, and 4 (rendering, UI Core, optimization) and the macOS standard UI components ([#15](https://github.com/simota/noa-gpui/issues/15)) are implemented. On Desktop and the Web (WebGPU / WebGL2), the following work.
>
> - Widget Tree, layout with Taffy, incremental updates, Hit Testing, Focus management
> - Component foundations: pointer shapes (an I-beam over text; resize cursors for dividers, column borders, and window edges and corners; a grabbing hand while dragging), themes (designs Flat + Vibrancy / Big Sur / Liquid Glass / Aero / Metro / Fluent 2 / Noa, light / dark, accent color, an increase-contrast setting), disabled state, overlays (menus, popovers, modals), animation (a reduce-motion setting), focus groups and shortcuts, text selection, undo, and clipboard, drag and drop, tooltips
> - Layout containers: Container, StackView, GridView, GroupBox, SplitView, TabView (`documents` gives editor- or browser-style document tabs: close buttons, a + button, reordering by drag and keyboard, an unsaved marker; `TabView::add_tab` / `close_tab` open and close a tab together with its page), DisclosureGroup, ScrollView, VisualEffectView
> - Text: Label (multi-line center and end alignment), TextField, SecureTextField, SearchField, TokenField, ComboBox, TextView, LogView (an ever-growing monospaced log, with per-line colors, search, and copy), Terminal (a VT100 / xterm screen; on desktop macOS and Linux, `ui_platform::Pty` connects it to a program in a pseudo-terminal)
> - Buttons: Button (the various macOS styles, default / cancel), Checkbox, RadioGroup, Switch, Disclosure, PopUpButton, PullDownButton, ComboButton, SegmentedControl
> - Value input and status display: Slider, Stepper, ColorWell, DatePicker (`DateFormat` picks the order of the date fields, the separators, and the 12-hour clock from the locale), PathControl, ProgressBar, Spinner, LevelIndicator, Image, SvgIcon (vector icons drawn from SVG paths or small SVG files; [#18](https://github.com/simota/noa-gpui/issues/18))
> - Additional input components: NumberInput (number parsing, clamping to a range, scrubbing, a built-in Stepper), ShortcutRecorder, FormField (label, required marker, help text, error message), InlineEditableLabel (also supports in-place renaming in OutlineView and CollectionView), FindBar (find and replace in TextView / TextArea, with case sensitivity, whole words, and regular expressions), range selection for Slider, period selection for DatePicker, leading and trailing accessories for TextField, completion lists for TokenField / ComboBox (dynamically fetched candidates, highlighting of the matched part, multiple selection) ([#19](https://github.com/simota/noa-gpui/issues/19))
> - Data display: TableView (showing and hiding columns from the header's right-click menu, `column_chooser` / `set_column_visible`; hidden columns are not drawn, are excluded from focus, copy, export, and filtering, and are included in the saved view state), OutlineView (sidebar; adding, removing, and moving items, reordering selected items by drag and moving them to another folder `ItemsMoved`, exporting the hierarchy with `items`), CollectionView (reordering selected items by drag), BrowserView, RuleEditor
> - Menus and things shown above windows: context menus, MenuBar, Toolbar, Alert, Sheet, Popover, Panel, color panel, font panel
> - Windows inside a window: Desktop, InnerWindow, WindowManager (move, resize, stacking order, snapping, dock, per-window modals) ([#16](https://github.com/simota/noa-gpui/issues/16))
> - Notifications and status display: Toast, NotificationBanner, Badge, Tag, Kbd, Skeleton, HoverCard, EmptyState, StatusBar ([#20](https://github.com/simota/noa-gpui/issues/20))
> - Navigation and commands: CommandPalette (⌘K / ⇧⌘P, fuzzy search; `RecentCommands` puts recently chosen commands first; `menu_commands` turns menu bar items, including submenus, directly into commands), AnchorNav (navigation linked to the scroll position), PathControl as breadcrumbs (long hierarchies collapse the middle into "…"), a navigation rail for SplitView (collapses the sidebar to icon width) ([#21](https://github.com/simota/noa-gpui/issues/21))
> - Layout container extensions: cards (a GroupBox style), Accordion (a mode with only one open at a time), Drawer (Sheet) sliding in from the window edge, wrapping and aspect ratio for Container, ResizableBox (shares resize handling with InnerWindow), pinning group rows to the top in TableView / OutlineView ([#22](https://github.com/simota/noa-gpui/issues/22))
> - Data display extensions: PropertyGrid (an inspector that lays out an editing control per type), DescriptionList (label and value pairs), a data grid for TableView (per-cell focus, range selection, in-place editing, frozen columns), following the end in ScrollView, FileDropZone (dropping files and opening a file dialog) ([#23](https://github.com/simota/noa-gpui/issues/23))
> - Charts and table data exchange: Chart (line, bar, stacked bar `stacked` (the axis fits the total, and the readout includes the total), sparkline; reading values by hover and arrow keys, choosing an item by click, live display by appending at the end and discarding old values, showing and hiding series by clicking the legend or with number keys), copying from TableView (⌘C copies the selected rows or cells as tab-separated text), exporting the whole displayed table as CSV / TSV (`TableView::export_text`; with a header row, following the current filter and sort) and importing with `TableText::read` (Export CSV… / Import CSV… in the demo's data grid), paste, cut, and clear in the data grid (exchanging cell ranges with spreadsheet software; a single value fills the whole selected range), fill down and right with ⌘D / ⌘R, undo and redo of cell changes (⌘Z / ⇧⌘Z; each edit, paste, cut, clear, and fill is one step), type-to-select (typing the start of a name selects a row), auto-scrolling at the view edge while dragging to reorder rows or select a cell range, truncating cells that do not fit the column width with "…" (end, middle, start; a truncated cell shows its full text in a tooltip)
> - CSS-like styling (Noa Style): selectors on Widget type, ID, class, and state, combinators, cascade and inheritance, `@media`, and variables declaratively override layout and rendering. Background, border, and corner radius work on any Widget (including custom Widgets), and text inputs can also change the border and text color and size. On desktop, the sheet is loaded from a file and reapplied on the spot every time it is saved (`App::watch_stylesheet`, the demo's `--stylesheet`) ([docs/styling.md](docs/styling.md))
> - Filtering: `set_filter_text` on TableView and OutlineView (shows only the rows and items that contain every word in the search field; case-insensitive, highlights the matched part, and keeps the order, the scroll position, and the selection of displayed rows; OutlineView expands the parents of matching items to show them, and when the filter is cleared it shows the selected item in its place), `set_filter` on TableView (column conditions; `RuleEditor::matcher` passes a rule editor's conditions as is)
> - Saving and restoring view state: what the user arranged by hand (table column widths, order, and hidden columns, SplitView divider positions and collapsed panes, selected tabs, expanded DisclosureGroups and OutlineView items, ScrollView positions, InnerWindow position, size, and maximization) is written to `ViewState` per name attached to a Widget (`UiTree::set_autosave_name`; corresponds to macOS's autosave name) and restored at the next launch. Windows opened later are also restored when they are given a name. On desktop, `App::autosave_view_state` handles reading and writing the file as well as the main window's size and position (the demo's `--view-state`)
>
> On desktop, the following are also supported.
>
> - IME (Japanese input)
> - The OS clipboard
> - Multiple windows (including always-on-top panels)
> - Accessibility through AccessKit
> - The OS menu bar (macOS), menu bar extras / the system tray (macOS, Windows, Linux; on Linux, desktops that can show StatusNotifierItem), open / save file dialogs
> - Windowless rendering (`Headless`): writes the Widget Tree or the screen to PNG. In environments without a GPU adapter, it renders on the CPU
>
> iOS and Android are not supported. IME, accessibility, and file dialogs on the Web are not supported yet.

## Target platforms

| Platform | Preferred GPU backend | Fallback |
| --- | --- | --- |
| Windows | Direct3D 12 | Vulkan |
| macOS | Metal | — |
| Linux | Vulkan | OpenGL |
| Web | WebGPU | WebGL2 |

iOS and Android are not targets (they were dropped from the targets on 2026-11-12).

## Design principles

1. Separate the GPU abstraction from the UI drawing commands (Display List).
2. Treat WebGL2 as the lowest common denominator, so that WebGPU-specific extensions can be added later.
3. Allow layout, text processing, and the render cache to be optimized independently of each other.

Only GPU rendering is built in-house; text shaping and layout use mature libraries. Text input (IME) and accessibility differ widely between OSes, so they are confined to a small Platform Bridge.

## Tech stack

| Layer | Technology |
| --- | --- |
| GPU abstraction | [wgpu](https://github.com/gfx-rs/wgpu) |
| Windows and input | [winit](https://github.com/rust-windowing/winit) |
| 2D rendering | Custom wgpu renderer (SDF rounded corners + Instanced Rendering) |
| Text | [cosmic-text](https://github.com/pop-os/cosmic-text) + [glyphon](https://github.com/grovesNL/glyphon) |
| Layout | [taffy](https://github.com/DioxusLabs/taffy) |
| Accessibility | [AccessKit](https://github.com/AccessKit/accesskit) + a Web-specific implementation |
| OS integration (desktop) | [tray-icon](https://github.com/tauri-apps/tray-icon) (menu bar and tray), [rfd](https://github.com/PolyMeilex/rfd) (file dialogs) |

## Structure

| crate | Role |
| --- | --- |
| `crates/ui-scene` | GPU-independent Display List, Clip, order-preserving batching, vector icons (SVG path parsing and rasterization) |
| `crates/ui-text` | Text shaping with cosmic-text and retention of layouts (bundles Inter) |
| `crates/ui-layout` | Adapter for Taffy (Flexbox / Grid) |
| `crates/ui-core` | Widget Tree (an Arena with Generational IDs), events, Hit Testing, Focus, Dirty Flags, themes, overlays, animation, text editing, drag and drop, accessibility, tooltips, and the parts corresponding to macOS standard components |
| `crates/ui-render-wgpu` | Three pipelines (Quad / Image / Text), the vector icon atlas, background blur, GPU initialization, windowless rendering (GPU offscreen, and CPU rendering for environments without a GPU) and PNG export |
| `crates/ui-platform` | The winit application. Windows, input translation, the frame loop, multiple windows, the clipboard, screen readers, system menus and the tray, file dialogs, wake-ups from other threads (`Waker`), pseudo-terminals (`Pty`), windowless rendering (`Headless`), stylesheet file watching (`StyleSheetWatcher`). Also the entry point that re-exports the other crates |
| `apps/demo` | Demo screens shared by desktop and the Web (an example of using `ui-platform`) |
| `apps/desktop` | The desktop executable `noa-demo` |
| `apps/web` | The Web Wasm (two variants, WebGPU and WebGL2) and the loader |

## Using it from your own app

Add only `ui-platform` as a dependency. On desktop, enable `native`; on the Web, enable exactly one of `webgpu` or `webgl2`.

```toml
[dependencies]
ui-platform = { git = "https://github.com/simota/noa-gpui", features = ["native"] }
```

A screen is a type that implements the `Screen` trait and holds the Widget Tree and the reactions to messages emitted by Widgets. Pass it to `App::new` and call `run` to open a window and run it. Crates such as `ui-core` are re-exported, as in `ui_platform::ui_core`. The minimal example is in `crates/ui-platform/examples/hello.rs`.

```sh
cargo run -p ui-platform --example hello --features native
cargo run -p ui-platform --example snapshot --features native -- snapshot.png   # Render to PNG without opening a window
```

On the Web, initialize the GPU from a canvas and then use `App::with_gpu` (`apps/web/src/lib.rs`).

## Running

Requires Rust 1.89 or later (a requirement of the cosmic-text 0.19 dependency).

### Desktop

```sh
cargo run -p desktop --release                                    # Widget Tree demo (a 1,000-row list)
cargo run -p desktop --release -- --count=10000                   # 10,000 rows
cargo run -p desktop --release -- --count=10000 --bench=600       # Measure 600 consecutive frames of scrolling and row updates
cargo run -p desktop --release -- --scene=catalog                 # UI catalog: built-in Widgets and their variations
cargo run -p desktop --release -- --scene=minios                  # Mini OS: a desktop inside a window, with small apps
cargo run -p desktop --release -- --scene=showcase                # Phase 1: showcase of rendering primitives
cargo run -p desktop --release -- --scene=stress --count=1500     # Phase 1: 1,500 Widgets built directly as a Display List
cargo run -p desktop --release -- --scene=catalog --snapshot=catalog.png   # Write the first frame to PNG and exit (no window or GPU needed)
cargo run -p desktop --release -- --scene=catalog --stylesheet=apps/demo/styles/example.noa.css   # Apply a Noa Style sheet and reapply it on every save
cargo run -p desktop --release -- --scene=minios --view-state=demo-state.txt   # Save column widths, dividers, window positions, and so on at exit, and restore them at the next launch
```

In the demo you can click buttons, move focus with Tab / Shift+Tab, activate with Enter / Space, type into text fields (on desktop, Japanese input through the IME also works), and scroll with the wheel. Frames with no changes are not rendered (0 FPS when idle).

The UI catalog (`--scene=catalog`) has the following 12 groups.

- **LAYOUT:** Layout containers (Container / Stack view / Grid view / Box & separator / Split view / Tab view / Disclosure group / Resizable box / ScrollView / Visual effect view)
- **TEXT:** Text display and input (Label / TextField / Secure field / Search field / Token field / Combo box / Text view / Log view / Terminal)
- **CONTROLS:** Buttons (Button / Checkbox / Radio group / Switch / Disclosure / Pop-up button / Pull-down & combo / Segmented control / Control size)
- **VALUES:** Value input (Slider / Stepper / Color well / Date picker / Path control)
- **INPUTS:** Additional input components (Number input / Shortcut recorder / Form field / Inline editing / Find bar / Ranges / Field affixes / Completions)
- **STATUS:** Progress and status display (Progress bar / Spinner / Level indicator / Image view / Vector icon)
- **NOTICES:** Notifications and status display (Toast / Notification banner / Badge and tag / Keycaps / Skeleton / Hover card / Empty state and status bar)
- **DATA:** Data display (Table view / Data grid / Charts / Inspector / Following the end / Outline view / Sidebar / Collection view / Browser / Rule editor)
- **MENUS:** Menus and toolbars (Context menu / Menu bar / Toolbar / Menu bar extra)
- **NAVIGATION:** Navigation and commands (Command palette / Breadcrumbs / Anchor navigation / Navigation rail)
- **PRESENTATION:** Things shown above windows (Alert / Sheet / Drawer / Popover / Tooltip / Panel / Standard panels / Inner windows)
- **FOUNDATION:** Component foundations (themes, disabled state, overlays, animation, keyboard, text editing, drag and drop, multiple windows)

On desktop, the open / save file panels use the OS's standard dialogs (`NSOpenPanel` / `NSSavePanel` on macOS). The Web version does not have them.

The catalog app's menus (File / View / Go / Help) appear in the OS menu bar on macOS desktop, and at the top of the window on Windows, Linux, and the Web. The desktop version also shows a menu bar extra (a system tray icon on Windows, a StatusNotifierItem icon on Linux; it does not appear on desktops without a supporting panel).

Each group is laid out section by section. The navigation on the left moves to each section, and scrolling the page keeps the navigation's selection in sync. The navigation items and the reorderable rows are custom Widgets defined in `apps/demo/src/catalog/`, and double as an example of implementing Widgets outside `ui-core`.

The mini OS (`--scene=minios`, `?scene=minios` on the Web) is a desktop inside a single window. It has a menu bar, windows inside the window, a dock for minimized windows, and an app launcher, with four apps built only from framework parts: Finder (a sidebar and file list, search within a folder), Notes (creating, editing, deleting, and full-text searching notes; the first line is the title), Calculator, and Settings (style, appearance, accent color) (`apps/demo/src/mini_os/`).

There is also a notification center that opens from the right edge of the desktop (incoming notifications appear as a Toast in the top right, and the unread count appears as a badge on the launcher; in the panel they can be dismissed one at a time or all at once), and an App Switcher that lists the open windows and switches between them ([#24](https://github.com/simota/noa-gpui/issues/24)). The App Switcher opens with ⌥Tab (Ctrl+Tab / Alt+Tab on Windows and Linux); while holding the key, choose with Tab / Shift+Tab, and release to switch. On macOS the OS takes ⌘Tab first, so ⌥Tab is used instead.

To check the display on Linux (X11) from macOS or elsewhere, you can use the Ubuntu desktop environment in Docker ([tools/linux-desktop](tools/linux-desktop/README.md)).

### Web

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
./apps/web/build.sh                                  # Output the WebGPU and WebGL2 versions to apps/web/dist
python3 -m http.server 8765 --directory apps/web/dist
```

Open `http://127.0.0.1:8765/`: if a WebGPU adapter can be obtained, the WebGPU version starts; otherwise, the WebGL2 version starts. If the WebGPU version fails to initialize, it also switches to the WebGL2 version on a new canvas. `?backend=webgl2` selects the version, and `scene` / `count` / `bench` can be given in the URL in the same form as on desktop (measurement results go to the console).

### Tests

```sh
./scripts/verify.sh          # fmt, clippy, tests, GPU tests
./scripts/verify.sh --full   # The above, plus the wasm clippy and benchmarks (same as CI)
cargo test --workspace --exclude web --features demo/native,ui-render-wgpu/native   # Tests only
```

The `ui-render-wgpu` tests actually render offscreen on the GPU and read the pixels back to verify them (a GPU adapter is required). The full output of a failed check goes to `.verify/<check-name>.log`. The command that runs only one feature's tests is in [docs/feature-map.md](docs/feature-map.md).

### Benchmarks (no GPU needed)

```sh
cargo bench -p ui-core --bench ui_tree                     # 10,000 rows (40,002 Widgets)
BENCH_ROWS=50000 cargo bench -p ui-core --bench ui_tree
```

Prints the p50 / p95 / p99 of `UiTree::prepare` (layout, repaint, Display List assembly) for scroll-only frames and for frames that change one row's text while scrolling.

```sh
cargo bench -p ui-core --bench table                       # A 100,000-row table
BENCH_ROWS=1000000 cargo bench -p ui-core --bench table
```

Prints the time of one frame, from input handling to `UiTree::prepare`, for scrolling, click selection, range selection, keyboard selection, and select-all in the table view. Fails if any frame exceeds 16ms.

```sh
cargo bench -p ui-core --bench desktop                     # 20 windows × 500 Widgets
BENCH_WINDOWS=50 BENCH_WIDGETS=1000 cargo bench -p ui-core --bench desktop
```

Prints the time of one frame for dragging a title bar, resizing from a corner, and bringing a window to the front, for windows inside a window. Fails if any frame exceeds 16ms.

```sh
cargo bench -p ui-core --bench styling                     # A stylesheet on 2,000 rows (8,101 Widgets)
BENCH_ROWS=10000 cargo bench -p ui-core --bench styling
```

For a stylesheet (type, class, ID, state, descendant and child selectors, `:nth-child`, `:not()`, `!important`), prints the time of `UiTree::set_stylesheet`, which re-resolves every Widget, and the time of one frame when the pointer moves to a different row every frame.

```sh
cargo bench -p ui-render-wgpu --features native --bench gpu_frame   # CPU / GPU time of a full repaint (GPU required)
```

## Documentation

- [Architecture design](docs/architecture.md)
- [Noa Style — CSS-like styling](docs/styling.md)
- [Feature map — where each feature's code and tests live](docs/feature-map.md)
- [Design of windows inside a window](docs/plans/inner-windows.md)
- [AGENTS.md — rules for agents and development](AGENTS.md)

## Roadmap

| Phase | Scope | Status | Issue |
| --- | --- | --- | --- |
| 1 | GPU Renderer MVP | Done | [#1](https://github.com/simota/noa-gpui/issues/1) |
| 2 | UI Core / Layout | Done | [#2](https://github.com/simota/noa-gpui/issues/2) |
| 3 | Mobile support | Canceled (iOS and Android were dropped from the targets) | [#3](https://github.com/simota/noa-gpui/issues/3) |
| 4 | Optimization | Done | [#4](https://github.com/simota/noa-gpui/issues/4) |
| — | Windows inside a window | Done | [#16](https://github.com/simota/noa-gpui/issues/16) |
| — | macOS standard UI components | Done | [#15](https://github.com/simota/noa-gpui/issues/15) ([#6](https://github.com/simota/noa-gpui/issues/6)–[#14](https://github.com/simota/noa-gpui/issues/14)) |

Desktop (Windows / macOS / Linux) comes first, followed by the Web.

## License

[MIT](LICENSE)
