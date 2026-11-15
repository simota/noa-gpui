mod alert;
mod anchor_nav;
mod appear;
mod badge;
mod banner;
mod browser;
mod button;
mod chart;
mod collection;
mod color;
mod command_palette;
mod completion;
mod container;
mod containers;
mod date_picker;
mod description_list;
mod desktop;
mod editable;
mod fields;
mod file_drop;
mod find;
mod form_field;
mod hover_card;
mod image;
mod inline_label;
mod kbd;
mod label;
mod level;
mod log_view;
mod menu;
mod menu_bar;
mod menu_buttons;
mod number_input;
mod outline;
mod panel;
mod panes;
mod path_control;
mod popover;
mod pressable;
mod progress;
mod property_grid;
mod resize;
mod rows;
mod rule_editor;
mod scroll_view;
mod scroller;
mod segmented;
mod sheet;
mod shortcut_recorder;
mod skeleton;
mod slider;
mod standard_panels;
mod status_bar;
mod stepper;
mod svg_icon;
mod table;
mod terminal;
mod text_area;
mod text_field;
mod text_view;
mod toast;
mod toggles;
mod token_field;
mod tone;
mod toolbar;
mod tooltip;
mod tsv;
mod vt;

pub use alert::{Alert, AlertResponse, AlertStyle};
pub use anchor_nav::{AnchorNav, AnchorSelected};
pub use badge::{Badge, Tag, TagRemoved, TagStyle};
pub use banner::{BannerAction, BannerClosed, NotificationBanner};
pub use browser::{BrowserPathChanged, BrowserView};
pub use button::{Button, ButtonClicked, ButtonStyle};
pub use chart::{
    Chart, ChartKind, ChartPointActivated, ChartSeries, ChartSeriesToggled, nice_ticks,
};
pub use collection::{CollectionItem, CollectionView};
pub use color::{ColorChanged, ColorPicker, ColorWell, from_hsv, hex_string, to_hsv};
pub use command_palette::{
    COMMAND_PALETTE_SHORTCUTS, CommandChosen, CommandPalette, FuzzyMatch, PaletteCommand,
    RecentCommands, fuzzy_match, menu_commands,
};
pub use completion::CompletionSource;
pub use container::Container;
pub use containers::{
    GridCell, GridPlacementMode, GridTrack, GridView, GroupBox, GroupBoxStyle, Material, Separator,
    StackAlignment, StackAxis, StackDistribution, StackView, VisualEffectView,
};
pub use date_picker::{
    DateChanged, DateElements, DateFormat, DateOrder, DatePicker, DatePickerStyle, DateRangeChanged,
};
pub use description_list::{DescriptionList, DescriptionListParts};
pub use desktop::{
    DESKTOP_DOCK, Desktop, DesktopCommand, DockClicked, InnerWindow, SNAP_DISTANCE,
    WINDOW_MIN_SIZE, WINDOW_TITLE_BAR, WindowEvent, WindowGesture, WindowManager, WindowPlacement,
};
pub use editable::{TextChanged, TextSubmitted};
pub use fields::{ComboBox, SearchField, SearchSubmitted, SecureTextField};
pub use file_drop::{FileDialogRequested, FileDropZone, FilesDropped};
pub use find::{
    FIND_SHORTCUT, FindAction, FindBar, FindError, FindOptions, FindStatus, drive_find,
    find_matches, replace_all,
};
pub use form_field::{FormField, LabelPosition};
pub use hover_card::HoverCard;
pub(crate) use hover_card::HoverCardPanel;
pub use image::{Image, ImageFrame, ImageScaling, image_rect};
pub use inline_label::{InlineEditableLabel, ItemRenamed, LabelEdited};
pub use kbd::Kbd;
pub use label::{Label, Truncation};
pub use level::{LEVEL_CRITICAL, LEVEL_NORMAL, LEVEL_WARNING, LevelIndicator, LevelStyle};
pub use log_view::{LogLine, LogMatch, LogView};
pub use menu::{Menu, MenuItem, menu_index, menu_item};
pub use menu_bar::{ContextMenuArea, MENU_BAR, MenuBar, MenuBarChosen, MenuBarMenu};
pub use menu_buttons::{ComboButton, PopUpButton, PullDownButton};
pub use number_input::{NumberInput, SCRUB_PIXELS, format_number, parse_number};
pub use outline::{
    ItemExpanded, ItemsMoved, OUTLINE_INDENT, OUTLINE_RAIL_WIDTH, OutlineItem, OutlineStyle,
    OutlineView,
};
pub use panel::{PANEL_TITLE_BAR, Panel, PanelClosed, PanelStyle};
pub use panes::{
    Accordion, DIVIDER, DOCUMENT_TAB_BAR, DisclosureGroup, NewTabRequested, PanesChanged,
    RAIL_DURATION, SplitPane, SplitView, TabCloseRequested, TabMoved, TabView,
};
pub use path_control::{PathControl, PathItem, PathItemClicked, PathSeparator, PathStyle};
pub use popover::{POPOVER_ARROW, Popover};
pub use pressable::Pressable;
pub use progress::{BAR_PERIOD, ProgressBar, SPINNER_PERIOD, Spinner};
pub use property_grid::{PropertyChanged, PropertyGrid, PropertyValue};
pub use resize::{BoxResized, ResizableBox, ResizeEdges};
pub use rows::{RowSelectionChanged, SelectionMode};
pub use rule_editor::{
    RULE_ROW, Rule, RuleEditor, RuleField, RuleKind, RuleMatch, RuleMatcher, RuleOperator,
    RulesChanged,
};
pub use scroll_view::{ScrollView, ScrollerStyle};
pub use scroller::{GESTURE_END, LEGACY_TRACK};
pub use segmented::{
    SEGMENT_UNDERLINE, SegmentClicked, SegmentMode, SegmentStyle, SegmentedControl, SegmentsChanged,
};
pub use sheet::{Sheet, SheetClosed};
pub use shortcut_recorder::{ShortcutChanged, ShortcutRecorder};
pub use skeleton::{SHIMMER_PERIOD, Skeleton};
pub use slider::{RangeChanged, Slider};
pub use standard_panels::{ColorPanel, FontChoice, FontPanel};
pub use status_bar::{EmptyState, EmptyStateParts, STATUS_BAR_HEIGHT, StatusBar, StatusBarParts};
pub use stepper::{REPEAT_DELAY, REPEAT_INTERVAL, Stepper};
pub use svg_icon::{Icon, SvgIcon};
pub use table::{
    CellActionClicked, CellButton, CellButtonClicked, CellEdited, CellLook, CellRange,
    CellSelectionChanged, ColumnAlign, ColumnResized, ColumnSort, ColumnVisibilityChanged,
    GroupActionClicked, GroupRowClicked, RowActivated, RowSeparator, RowsMoved, SortChanged,
    TABLE_CELL_PADDING, TABLE_CHECK_COLUMN, TABLE_HEADER, TABLE_ROW, TableColumn, TableData,
    TableView,
};
pub use terminal::{Terminal, TerminalInput};
pub use text_area::TextArea;
pub use text_field::TextField;
pub use text_view::TextView;
pub use toast::{TOAST_DURATION, TOAST_WIDTH, Toast, ToastAction, ToastClosed};
pub use toggles::{
    CheckState, Checkbox, CheckboxChanged, Disclosure, DisclosureStyle, RadioGroup, Switch,
};
pub use token_field::{TokenField, TokensChanged};
pub use tone::Tone;
pub use toolbar::{Toolbar, ToolbarDisplay, ToolbarItem, ToolbarItemClicked, ToolbarSymbol};
pub use tooltip::Tooltip;
pub use tsv::TableText;
pub use vt::{CellStyle as TerminalCellStyle, TermColor, TerminalCell};

use noa_gpui_scene::{Border, Color, Rect, Size, TextLayoutId};
use noa_gpui_text::{TextStore, TextStyle};

/// The four macOS control sizes; metrics and text scale from the regular size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlSize {
    Mini,
    Small,
    #[default]
    Regular,
    Large,
}

impl ControlSize {
    pub fn scale(self) -> f32 {
        match self {
            ControlSize::Mini => 0.75,
            ControlSize::Small => 0.86,
            ControlSize::Regular => 1.0,
            ControlSize::Large => 1.15,
        }
    }

    pub(crate) fn font(self, base: f32) -> f32 {
        (base * self.scale()).round()
    }
}

/// Emitted when a control's single selection changes: [`RadioGroup`], [`PopUpButton`],
/// [`SegmentedControl`] in [`SegmentMode::SelectOne`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionChanged(pub usize);

/// Emitted when an on / off control changes: [`Switch`], [`Disclosure`] (expanded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toggled(pub bool);

/// Emitted when a [`Slider`], [`Stepper`] or editable [`LevelIndicator`] changes value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueChanged(pub f64);

/// Emitted when an action is chosen from the menu of a [`PullDownButton`],
/// [`ComboButton`] or [`ContextMenuArea`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuChosen(pub usize);

/// Set the height of a scrolling view: `Some` fixes it, `None` fills the space the
/// parent has left along a column (or the cross axis of a row), down to `min`.
pub(crate) fn view_height(style: &mut noa_gpui_layout::Style, height: Option<f32>, min: f32) {
    use noa_gpui_layout::style::{auto, length};
    match height {
        Some(height) => {
            style.size.height = length(height);
            style.flex_shrink = 0.0;
        }
        None => {
            style.size.height = auto();
            style.min_size.height = length(min);
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = length(0.0);
        }
    }
}

/// The keyboard focus ring around `rect` (widget coordinates), in the theme's
/// [`Role::FocusRing`](crate::theme::Role::FocusRing), which reaches 3:1 against the
/// background (see [`crate::Theme::with_accent`]).
pub(crate) fn focus_ring(cx: &mut crate::widget::PaintCx, rect: Rect, radius: f32) {
    let ring = cx.color(crate::theme::Role::FocusRing);
    cx.rounded_rect(
        Rect::new(
            rect.x - 3.0,
            rect.y - 3.0,
            rect.width + 6.0,
            rect.height + 6.0,
        ),
        radius + 3.0,
        Color::TRANSPARENT,
        Border {
            width: 3.0,
            color: ring,
        },
    );
}

/// How much longer along its track than across a knob (switch, slider) is drawn:
/// Liquid Glass stretches knobs into capsules, Metro thins them into bars, Noa into
/// blocks the shape of the cursor `▮`.
pub(crate) fn knob_stretch(cx: &crate::widget::PaintCx) -> f32 {
    match cx.theme().design {
        crate::theme::Design::LiquidGlass => 1.5,
        crate::theme::Design::Metro => 0.5,
        crate::theme::Design::Noa => 0.6,
        _ => 1.0,
    }
}

/// `fill` darkened (or lightened, in dark mode) for a pressed control.
pub(crate) fn pressed_tint(cx: &crate::widget::PaintCx, fill: Color) -> Color {
    crate::theme::mix(fill, cx.color(crate::theme::Role::Text), 0.12)
}

/// A text layout reshaped only when its string, style or wrap width changes.
#[derive(Debug, Default)]
pub(crate) struct CachedText {
    layout: Option<TextLayoutId>,
    key: Option<(String, Option<u32>)>,
    size: Size,
}

impl CachedText {
    pub fn shape(
        &mut self,
        store: &mut TextStore,
        text: &str,
        style: TextStyle,
        wrap: Option<f32>,
    ) -> Size {
        let wrap_bits = wrap.map(f32::to_bits);
        // Compare in place: measuring runs several times per layout, and an unchanged
        // key must not allocate.
        let unchanged = self.layout.is_some()
            && self
                .key
                .as_ref()
                .is_some_and(|(k, w)| k == text && *w == wrap_bits);
        if !unchanged {
            self.reshape(store, text, style, wrap);
        }
        self.size
    }

    #[inline(never)]
    fn reshape(&mut self, store: &mut TextStore, text: &str, style: TextStyle, wrap: Option<f32>) {
        let wrap_bits = wrap.map(f32::to_bits);
        // Only the wrap width changed, as when a layout pass asks for a wrapping text's
        // narrowest and widest size before its real one: its glyphs stay.
        if let (Some(id), Some((shaped, shaped_wrap))) = (self.layout, &mut self.key)
            && shaped == text
            && store.rewrap(id, style, wrap)
        {
            *shaped_wrap = wrap_bits;
            self.size = store.size(id);
            return;
        }
        self.release(store);
        let id = store.create(text, style, wrap);
        self.size = store.size(id);
        self.layout = Some(id);
        self.key = Some((text.to_owned(), wrap_bits));
    }

    /// Take over `id`, a layout of `text` that `store.create(text, style, wrap)` made,
    /// as if [`CachedText::shape`] had shaped it.
    pub fn adopt(
        &mut self,
        store: &mut TextStore,
        id: TextLayoutId,
        text: &str,
        wrap: Option<f32>,
    ) {
        self.release(store);
        self.size = store.size(id);
        self.layout = Some(id);
        self.key = Some((text.to_owned(), wrap.map(f32::to_bits)));
    }

    pub fn layout(&self) -> Option<TextLayoutId> {
        self.layout
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn release(&mut self, store: &mut TextStore) {
        if let Some(id) = self.layout.take() {
            store.remove(id);
        }
    }
}
