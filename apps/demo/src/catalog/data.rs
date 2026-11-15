use std::borrow::Cow;

use noa_gpui_core::style::{AlignItems, FlexDirection, Size as LayoutSize, auto, length, percent};
use noa_gpui_core::widgets::{
    Aggregate, BrowserPathChanged, BrowserView, Button, ButtonClicked, CellActionClicked,
    CellButton, CellButtonClicked, CellEdited, CellLook, CellSelectionChanged, Chart,
    ChartPointActivated, ChartSeries, CollectionItem, CollectionView, ColumnAlign, ColumnSort,
    ColumnVisibilityChanged, Container, DescriptionList, DescriptionListParts, GroupActionClicked,
    GroupRowClicked, Grouping, ItemExpanded, ItemRenamed, ItemsMoved, Label, NumberInput,
    OutlineItem, OutlineView, PageRequest, PropertyGrid, PropertyValue, RemoteRows, RowActivated,
    RowSelectionChanged, RowSeparator, RowsMoved, RuleEditor, RuleField, RulesChanged, ScrollView,
    SearchField, SortChanged, TableColumn, TableData, TableView, TextChanged, Tone, from_hsv,
    hex_string,
};
use noa_gpui_core::{Message, Role, Style, UiTree, WidgetId};
use noa_gpui_scene::{Color, ImageId, VectorIcon};

use super::kit::{Kit, gap, row_style};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 12] = [
    SectionSpec {
        title: "Table view",
        description: "Columns with headers: click a header to sort (again to reverse), drag \
                      its edge to resize. Click, Cmd-click and Shift-click select rows; arrows, \
                      Page Up / Down and Home / End move the selection, Cmd+A selects all. Only \
                      the rows on screen are drawn, so this table of 100,000 rows scrolls and \
                      selects as fast as one of 100. Type in the search field to show only \
                      the rows with those words (matches are marked; the group rows of the \
                      pantry stay over their matches). Type the start of a name to jump to it; \
                      Cmd+C copies the selected rows as tab-separated text that spreadsheets \
                      read. Rows can also be dragged into a new order. \
                      Group rows head the rows below them and stay pinned to the top while \
                      their group scrolls by.",
        build: tables,
    },
    SectionSpec {
        title: "Data grid",
        description: "A table that focuses cells instead of rows. Click a cell or move with \
                      the arrows (Home / End: first and last column); Shift or a drag selects \
                      a rectangle, Cmd+A all of it. The month columns are editable: Return or \
                      a double click edits the cell in place, typing replaces it, Escape \
                      cancels. Cmd+C / Cmd+X / Cmd+V copy, cut and paste blocks of cells as \
                      tab-separated text (to and from a spreadsheet; one value fills the \
                      selected range) and Delete clears them. Cmd+D / Cmd+R fill the \
                      selection down / right from its first row / column. Cmd+Z takes the last change \
                      back, Shift+Cmd+Z does it again. The region column is frozen and \
                      stays put while the months scroll sideways under it. The chart follows \
                      the focused region and every edit; click a month on it to go to its cell. \
                      On the desktop, Export CSV… saves the grid for a spreadsheet and Import \
                      CSV… loads one back (a header row is skipped).",
        build: data_grid,
    },
    SectionSpec {
        title: "Charts",
        description: "Values as lines or bars against an axis with round steps. Hover, or \
                      focus a chart and use the arrows, to read a month's values; click (or \
                      Return) to act on it. Click a legend entry (or press its number) to \
                      hide or show that series. A live chart takes samples as they come and keeps \
                      the last 30; sparklines are the bare lines, for a row of figures.",
        build: charts,
    },
    SectionSpec {
        title: "Inspector",
        description: "PropertyGrid lines property names up with a control for each type: a \
                      number input, a color well, a switch and a pop-up for a choice. \
                      DescriptionList lines labels up with their values (a GridView of two \
                      columns); this one follows the inspector.",
        build: inspector,
    },
    SectionSpec {
        title: "Following the end",
        description: "A scroll view that follows its end stays at the bottom as content is \
                      added, as a log or a streamed chat does. Scroll up to read and it stays \
                      where you are; scroll back to the bottom (or Jump to End) and it \
                      follows again.",
        build: follow_end,
    },
    SectionSpec {
        title: "Outline view",
        description: "A hierarchy of rows. Click a triangle to expand or collapse (Option: the \
                      whole branch); Right and Left do the same from the keyboard, or step to \
                      the first child and back to the parent. Emits ItemExpanded. Type in the \
                      search field to list only the items with those words, inside their \
                      opened folders; pick one and clear the field to see it in its place. \
                      Drag the selected items to move them: onto the top or bottom of a row \
                      to put them before or after it, onto its middle to put them inside \
                      (Option+Cmd+Up / Down move them from the keyboard). \
                      New Folder adds one next to the selection and names it in place \
                      (Return renames the selected item too); Delete removes the selection.",
        build: outline,
    },
    SectionSpec {
        title: "Sidebar",
        description: "The outline view as a source list: section headings that hide and show \
                      their items, icons, badges and a rounded selection. Down and Up skip the \
                      headings, and the heading of the section being scrolled through stays \
                      at the top. Drag an item to reorder it, or into another section.",
        build: sidebar,
    },
    SectionSpec {
        title: "Collection view",
        description: "A grid of items that wraps to the width. Click, Cmd-click and \
                      Shift-click select; drag across empty space to select a rectangle; \
                      arrows move by item and by row. Drag the selected swatches to put them \
                      somewhere else (a line shows where; held near the top or bottom, the \
                      grid scrolls), or move them with Option+Cmd+arrows. 5,000 items, drawn a row at a time as they scroll into \
                      view.",
        build: collection,
    },
    SectionSpec {
        title: "Browser",
        description: "A hierarchy as columns, like Finder's column view: selecting an item \
                      lists its children in the next column. Up / Down within a column, Right \
                      into the children, Left back. Columns scroll sideways to follow.",
        build: browser,
    },
    SectionSpec {
        title: "Rule editor",
        description: "Search criteria built row by row: a field, an operator chosen by the \
                      field's kind (text or number) and a value. − and + remove and add rows; \
                      All / Any combines them. The table below is filtered in place as you \
                      edit: its sort and the selected rows that still match stay.",
        build: rules,
    },
    SectionSpec {
        title: "Grouping",
        description: "Grouping::by(column) wraps any table data: a group row before each \
                      group with its row count and aggregates (sum, average, min, max) in \
                      their columns. Click a group row to fold or unfold it; sorting \
                      orders the rows within their groups.",
        build: grouping,
    },
    SectionSpec {
        title: "Remote rows",
        description: "A million rows that live on a (simulated) server, loaded a page at a \
                      time as they scroll into view; rows on their way show placeholders. \
                      Sorting is the server's: a header click asks for the rows again in \
                      the new order.",
        build: remote,
    },
];

fn status_label(kit: &mut Kit, parent: WidgetId, text: &str) -> WidgetId {
    kit.tree.insert(
        Some(parent),
        Label::new(text).font_size(12.0).color(Role::Muted),
    )
}

fn set_status(cx: &mut DemoCx, label: WidgetId, text: String) {
    cx.tree
        .update::<Label, _>(label, |l, cx| l.set_text(cx, text));
}

/// A pantry in groups: a group row, then its items.
struct Pantry(Vec<(String, Option<u32>)>);

impl Pantry {
    fn new() -> Self {
        let groups: [(&str, &[&str]); 4] = [
            (
                "Fruit",
                &["Apples", "Bananas", "Cherries", "Figs", "Lemons", "Pears"],
            ),
            (
                "Vegetables",
                &["Carrots", "Kale", "Leeks", "Onions", "Peppers"],
            ),
            ("Grains", &["Barley", "Oats", "Rice", "Rye", "Wheat"]),
            ("Spices", &["Cumin", "Nutmeg", "Paprika", "Saffron"]),
        ];
        let mut rows = Vec::new();
        for (g, (group, items)) in groups.into_iter().enumerate() {
            rows.push((group.to_owned(), None));
            for (i, item) in items.iter().enumerate() {
                let stock = ((g * 7 + i) * 37 % 90 + 3) as u32;
                rows.push(((*item).to_owned(), Some(stock)));
            }
        }
        Self(rows)
    }
}

impl TableData for Pantry {
    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        let (name, stock) = &self.0[row];
        match (column, stock) {
            (0, _) => Cow::Borrowed(name),
            (_, Some(stock)) => Cow::Owned(stock.to_string()),
            (_, None) => Cow::Borrowed(""),
        }
    }

    fn is_group_row(&self, row: usize) -> bool {
        self.0[row].1.is_none()
    }
}

/// Rows are computed on demand; nothing is stored per row.
struct Files(usize);

const KINDS: [&str; 5] = ["Document", "Image", "Movie", "Folder", "Archive"];

impl Files {
    fn size(row: usize) -> usize {
        (row * 7919 + 13) % 250_000
    }
}

impl TableData for Files {
    fn row_count(&self) -> usize {
        self.0
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        Cow::Owned(match column {
            0 => format!("File {row:06}"),
            1 => format!("{} KB", Self::size(row)),
            2 => KINDS[row % KINDS.len()].to_owned(),
            _ => format!("2026-{:02}-{:02}", row % 12 + 1, row % 28 + 1),
        })
    }
}

/// A table tall enough for the header and the six container rows.
const TABLE_HEADER_ROWS: f32 =
    noa_gpui_core::widgets::TABLE_HEADER + 6.0 * noa_gpui_core::widgets::TABLE_ROW + 1.0;

struct Services;

const SERVICES: [(&str, Tone, &str); 4] = [
    ("api", Tone::Success, "running"),
    ("worker", Tone::Success, "running"),
    ("scheduler", Tone::Warning, "degraded"),
    ("mailer", Tone::Critical, "stopped"),
];

impl TableData for Services {
    fn row_count(&self) -> usize {
        SERVICES.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        let (name, _, status) = SERVICES[row];
        Cow::Borrowed(match column {
            0 => name,
            1 => status,
            _ => "Restart",
        })
    }

    fn cell_look(&self, row: usize, column: usize) -> CellLook {
        match column {
            0 => CellLook::Colored(Tone::Accent),
            1 => CellLook::Badge(SERVICES[row].1),
            _ => CellLook::Button,
        }
    }
}

const PLAY: &str = r#"<svg viewBox="0 0 24 24"><path d="M7 4.5v15l12-7.5z"/></svg>"#;
const STOP: &str =
    r#"<svg viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" rx="1.5"/></svg>"#;
const RESTART: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M20 12a8 8 0 1 1-2.3-5.6"/><path d="M20 4v5h-5"/></svg>"#;
const TRASH: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/></svg>"#;

struct Containers {
    play: VectorIcon,
    stop: VectorIcon,
    restart: VectorIcon,
    trash: VectorIcon,
}

/// Name, ID, ports, running, and whether the row is a stack.
const CONTAINERS: [(&str, &str, &str, bool, bool); 6] = [
    ("shop", "", "", true, true),
    ("shop-web-1", "3f9a1c2b7d10", "8080→80/tcp", true, false),
    ("shop-db-1", "a71e0c55b3f2", "5432/tcp", true, false),
    ("blog", "", "", false, true),
    ("blog-app-1", "0c2d9e41aa87", "3000→3000/tcp", false, false),
    ("blog-cache-1", "e5b8f3016c4d", "6379/tcp", false, false),
];

impl Containers {
    fn load() -> Self {
        let svg = |s: &str| VectorIcon::from_svg(s).expect("the table's icons are valid SVG");
        Self {
            play: svg(PLAY),
            stop: svg(STOP),
            restart: svg(RESTART),
            trash: svg(TRASH),
        }
    }
}

impl TableData for Containers {
    fn row_count(&self) -> usize {
        CONTAINERS.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        let (name, id, ports, running, stack) = CONTAINERS[row];
        Cow::Borrowed(match column {
            0 => name,
            1 => id,
            2 => ports,
            3 if stack && running => "2/2 up",
            3 if stack => "0/2 up",
            3 if running => "running",
            3 => "exited",
            _ => "",
        })
    }

    fn is_group_row(&self, row: usize) -> bool {
        CONTAINERS[row].4
    }

    fn cell_look(&self, row: usize, column: usize) -> CellLook {
        let (_, _, _, running, stack) = CONTAINERS[row];
        match column {
            3 if stack => CellLook::Text,
            3 if running => CellLook::OutlinedBadge(Tone::Success),
            3 => CellLook::OutlinedBadge(Tone::Neutral),
            4 => CellLook::Buttons,
            _ => CellLook::Text,
        }
    }

    fn cell_buttons(&self, row: usize, _column: usize) -> Vec<CellButton> {
        let (_, _, _, running, stack) = CONTAINERS[row];
        if stack {
            return vec![
                CellButton::new("up").label("Up").enabled(!running),
                CellButton::new("stop").label("Stop").enabled(running),
            ];
        }
        let mut buttons = if running {
            vec![
                CellButton::new("stop")
                    .icon(self.stop.clone())
                    .tooltip("Stop"),
                CellButton::new("restart")
                    .icon(self.restart.clone())
                    .tooltip("Restart"),
            ]
        } else {
            vec![
                CellButton::new("start")
                    .icon(self.play.clone())
                    .tone(Tone::Success)
                    .tooltip("Start"),
            ]
        };
        buttons.push(CellButton::new("logs").label("Logs"));
        buttons.push(
            CellButton::new("delete")
                .icon(self.trash.clone())
                .tone(Tone::Critical)
                .tooltip("Delete")
                .enabled(!running),
        );
        buttons
    }
}

struct StackList {
    /// Indices into [`CONTAINERS`] of the rows shown, in order.
    rows: Vec<usize>,
    collapsed: Vec<bool>,
}

impl StackList {
    fn new(collapsed: Vec<bool>) -> Self {
        let mut rows = Vec::new();
        let mut hidden = false;
        for (i, &(.., stack)) in CONTAINERS.iter().enumerate() {
            if stack {
                hidden = collapsed[i];
                rows.push(i);
            } else if !hidden {
                rows.push(i);
            }
        }
        Self { rows, collapsed }
    }
}

impl TableData for StackList {
    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        let i = self.rows[row];
        let (name, _, ports, running, stack) = CONTAINERS[i];
        match column {
            0 if stack => {
                let fold = if self.collapsed[i] { "▸" } else { "▾" };
                Cow::Owned(format!("{fold} {name}"))
            }
            0 if running => Cow::Borrowed("running"),
            0 => Cow::Borrowed("exited"),
            1 if !stack => Cow::Borrowed(name),
            2 if !stack => Cow::Borrowed(ports),
            _ => Cow::Borrowed(""),
        }
    }

    fn is_group_row(&self, row: usize) -> bool {
        CONTAINERS[self.rows[row]].4
    }

    fn cell_detail(&self, row: usize, column: usize) -> Option<Cow<'_, str>> {
        let (_, id, ..) = CONTAINERS[self.rows[row]];
        (column == 1 && !id.is_empty()).then_some(Cow::Borrowed(id))
    }

    fn cell_note(&self, row: usize, column: usize) -> Option<Cow<'_, str>> {
        let i = self.rows[row];
        if column != 0 || !CONTAINERS[i].4 {
            return None;
        }
        let (running, total) = stack_counts(i);
        Some(Cow::Owned(format!("{running}/{total} running")))
    }

    fn cell_dot(&self, row: usize, column: usize) -> Option<Tone> {
        let i = self.rows[row];
        if column != 0 || !CONTAINERS[i].4 {
            return None;
        }
        Some(match stack_counts(i) {
            (0, _) => Tone::Neutral,
            (running, total) if running < total => Tone::Warning,
            _ => Tone::Success,
        })
    }

    fn cell_look(&self, row: usize, column: usize) -> CellLook {
        match (column, CONTAINERS[self.rows[row]].3) {
            (0, true) => CellLook::StrongOutlinedBadge(Tone::Success),
            (0, false) => CellLook::StrongOutlinedBadge(Tone::Neutral),
            _ => CellLook::Text,
        }
    }

    fn group_row_buttons(&self, row: usize) -> Vec<CellButton> {
        let running = CONTAINERS[self.rows[row]].3;
        let small =
            |action: &'static str, label: &str| CellButton::new(action).label(label).height(22.0);
        vec![
            small("up", "Up").enabled(!running),
            small("start", "Start").enabled(!running),
            small("restart", "Restart").enabled(running),
            small("stop", "Stop").enabled(running),
            small("down", "Down").tone(Tone::Critical),
        ]
    }

    fn row_separator(&self, row: usize) -> RowSeparator {
        let (.., running, stack) = CONTAINERS[self.rows[row]];
        if stack && !running {
            RowSeparator::Dashed
        } else {
            RowSeparator::Line
        }
    }
}

/// `(running, total)` containers of the stack at index `stack` in [`CONTAINERS`].
fn stack_counts(stack: usize) -> (usize, usize) {
    let members = CONTAINERS[stack + 1..].iter().take_while(|c| !c.4);
    members.fold((0, 0), |(running, total), c| {
        (running + usize::from(c.3), total + 1)
    })
}

struct TableDemo {
    table: WidgetId,
    search: WidgetId,
    status: WidgetId,
    pantry: WidgetId,
    pantry_search: WidgetId,
    list: WidgetId,
    list_status: WidgetId,
    tasks: Vec<&'static str>,
    services: WidgetId,
    services_status: WidgetId,
    containers: WidgetId,
    containers_status: WidgetId,
    stack_list: WidgetId,
    stack_list_status: WidgetId,
    collapsed: Vec<bool>,
}

impl TableDemo {
    fn describe(&self, cx: &DemoCx) -> String {
        let table = cx.tree.get::<TableView>(self.table).unwrap();
        let sort = match table.sort_column() {
            Some((c, asc)) => format!(
                " · sorted by {} {}",
                table.columns()[c].title(),
                if asc { "↑" } else { "↓" }
            ),
            None => String::new(),
        };
        let rows = if table.is_filtered() {
            format!(
                "{} of {} rows",
                table.matching_row_count(),
                table.total_row_count()
            )
        } else {
            format!("{} rows", table.row_count())
        };
        format!("{rows} · {} selected{sort}", table.selected_count())
    }
}

impl Demo for TableDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.search || message.source == self.pantry_search {
            if let Some(TextChanged(query)) = message.downcast_ref() {
                let table = if message.source == self.search {
                    self.table
                } else {
                    self.pantry
                };
                cx.tree
                    .update::<TableView, _>(table, |t, cx| t.set_filter_text(cx, query));
                if table == self.table {
                    let text = self.describe(cx);
                    set_status(cx, self.status, text);
                }
            }
            return true;
        }
        if message.source == self.table {
            let mut text = self.describe(cx);
            if let Some(RowActivated(row)) = message.downcast_ref() {
                text = format!("{text} · opened File {row:06}");
            }
            if let Some(ColumnVisibilityChanged { column, visible }) = message.downcast_ref() {
                let title = cx.tree.get::<TableView>(self.table).unwrap().columns()[*column]
                    .title()
                    .to_owned();
                let shown = if *visible { "shown" } else { "hidden" };
                text = format!("{text} · {title} {shown}");
            }
            if message.downcast_ref::<RowSelectionChanged>().is_some()
                || message.downcast_ref::<SortChanged>().is_some()
                || message.downcast_ref::<RowActivated>().is_some()
                || message.downcast_ref::<ColumnVisibilityChanged>().is_some()
            {
                set_status(cx, self.status, text);
            }
            return true;
        }
        if message.source == self.services {
            if let Some(CellButtonClicked { row, .. }) = message.downcast_ref() {
                let text = format!("restart {}", SERVICES[*row].0);
                set_status(cx, self.services_status, text);
            }
            return true;
        }
        if message.source == self.containers {
            if let Some(CellActionClicked { row, action, .. }) = message.downcast_ref() {
                let text = format!("{action} {}", CONTAINERS[*row].0);
                set_status(cx, self.containers_status, text);
            }
            return true;
        }
        if message.source == self.stack_list {
            let rows = StackList::new(self.collapsed.clone()).rows;
            if let Some(GroupRowClicked(row)) = message.downcast_ref() {
                let i = rows[*row];
                self.collapsed[i] = !self.collapsed[i];
                let data = StackList::new(self.collapsed.clone());
                cx.tree
                    .update::<TableView, _>(self.stack_list, |t, cx| t.set_data(cx, data));
            }
            if let Some(GroupActionClicked { row, action }) = message.downcast_ref() {
                let text = format!("{action} {}", CONTAINERS[rows[*row]].0);
                set_status(cx, self.stack_list_status, text);
            }
            return true;
        }
        if message.source == self.list {
            if let Some(RowsMoved { rows, to }) = message.downcast_ref() {
                let order = cx.tree.get::<TableView>(self.list).unwrap();
                let names: Vec<&str> = order
                    .displayed_rows()
                    .iter()
                    .map(|&r| self.tasks[r])
                    .collect();
                let text = format!("moved {} to {to}: {}", rows.len(), names.join(", "));
                set_status(cx, self.list_status, text);
            }
            return true;
        }
        false
    }
}

fn tables(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "TableView::new(columns, data) · TableColumn::new(..).sort(ColumnSort::Number) · \
         .set_filter_text(query) · .column_chooser(true): right-click the header",
        720.0,
    );
    let search = kit.tree.insert(
        Some(card),
        SearchField::new("Filter: words in any column, e.g. image 0042", 320.0),
    );
    kit.tree.set_accessible_label(search, "Filter files");
    let columns = vec![
        TableColumn::new("Name", 200.0)
            .sort(ColumnSort::Text)
            .hideable(false),
        TableColumn::new("Size", 120.0)
            .sort(ColumnSort::Number)
            .align(ColumnAlign::Trailing),
        TableColumn::new("Kind", 140.0).sort(ColumnSort::Text),
        TableColumn::new("Modified", 160.0),
    ];
    let table = kit.tree.insert(
        Some(card),
        TableView::new(columns, Files(100_000))
            .column_chooser(true)
            .height(320.0),
    );
    kit.tree.set_accessible_label(table, "Files");
    // Column widths, the sort and the columns hidden from the header's menu are
    // restored next time (with --view-state).
    kit.tree.set_autosave_name(table, "catalog.files");
    let status = status_label(kit, card, "100000 rows · 0 selected");

    let small = kit.card(".reorderable(true) · .alternating(false)");
    let tasks = vec![
        "Plan", "Design", "Build", "Test", "Review", "Ship", "Measure",
    ];
    let rows: Vec<Vec<String>> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| vec![format!("{}", i + 1), (*t).to_owned()])
        .collect();
    let list = kit.tree.insert(
        Some(small),
        TableView::new(
            vec![
                TableColumn::new("#", 40.0).resizable(false),
                TableColumn::new("Task", 200.0),
            ],
            rows,
        )
        .reorderable(true)
        .alternating(false)
        .height(200.0),
    );
    kit.tree.set_accessible_label(list, "Tasks");
    let list_status = status_label(kit, small, "drag rows to reorder");

    let grouped = kit.card(
        "TableData::is_group_row(row) · .floats_group_rows(true) · .selection_checkboxes(true)",
    );
    let pantry_search = kit
        .tree
        .insert(Some(grouped), SearchField::new("Filter the pantry", 240.0));
    kit.tree
        .set_accessible_label(pantry_search, "Filter pantry");
    let pantry = kit.tree.insert(
        Some(grouped),
        TableView::new(
            vec![
                TableColumn::new("Item", 150.0).sort(ColumnSort::Text),
                TableColumn::new("Stock", 90.0)
                    .sort(ColumnSort::Number)
                    .align(ColumnAlign::Trailing),
            ],
            Pantry::new(),
        )
        .selection_checkboxes(true)
        .height(220.0),
    );
    kit.tree.set_accessible_label(pantry, "Pantry");
    let looks = kit.card("TableData::cell_look → CellLook::{Colored, Badge, Button}");
    let services = kit.tree.insert(
        Some(looks),
        TableView::new(
            vec![
                TableColumn::new("Service", 110.0),
                TableColumn::new("Status", 100.0).align(ColumnAlign::Center),
                TableColumn::new("", 90.0).align(ColumnAlign::Center),
            ],
            Services,
        )
        .height(130.0),
    );
    kit.tree.set_accessible_label(services, "Services");
    let services_status = status_label(kit, looks, "click a Restart button");

    let stacks = kit.wide_card(
        "CellLook::{Buttons, OutlinedBadge} · TableColumn::flex / font_family · \
         .group_row_cells(true) · .group_member_indent(14) · .highlights_hovered_row(true)",
        720.0,
    );
    let containers = kit.tree.insert(
        Some(stacks),
        TableView::new(
            vec![
                TableColumn::new("Name", 140.0).flex(1.0),
                TableColumn::new("ID", 110.0).font_family("monospace"),
                TableColumn::new("Ports", 120.0).font_family("monospace"),
                TableColumn::new("Status", 90.0)
                    .font_family("monospace")
                    .flex(0.5),
                TableColumn::new("", 170.0).align(ColumnAlign::Trailing),
            ],
            Containers::load(),
        )
        .group_row_cells(true)
        .group_member_indent(14.0)
        .highlights_hovered_row(true)
        .alternating(false)
        .header_uppercase(true)
        .header_font_family("monospace")
        .header_background(Role::SurfaceAlt)
        .selection_mode(noa_gpui_core::widgets::SelectionMode::Single)
        .height(TABLE_HEADER_ROWS),
    );
    kit.tree.set_accessible_label(containers, "Containers");
    let containers_status = status_label(kit, stacks, "click a button in a row");

    let list_card = kit.wide_card(
        ".bordered(false) · .background(..) · .row_separators(true) · .content_inset(16) · \
         .group_member_indent_column(1) · TableData::{cell_detail, cell_note, cell_dot, \
         group_row_buttons, row_separator} · CellButton::height · \
         CellLook::StrongOutlinedBadge · TableColumn::detail_font_family · GroupRowClicked",
        720.0,
    );
    let collapsed = vec![false; CONTAINERS.len()];
    let stack_list = kit.tree.insert(
        Some(list_card),
        TableView::new(
            vec![
                TableColumn::new("Status", 100.0).font_family("monospace"),
                TableColumn::new("Container", 200.0)
                    .flex(1.0)
                    .detail_font_family("monospace"),
                TableColumn::new("Ports", 140.0).font_family("monospace"),
            ],
            StackList::new(collapsed.clone()),
        )
        .content_inset(16.0)
        .group_member_indent(36.0)
        .group_member_indent_column(1)
        .bordered(false)
        // The card's color, so the table reads as a list on it.
        .background(Role::Surface)
        .row_separators(true)
        .alternating(false)
        .floats_group_rows(false)
        .row_height(38.0)
        .selection_mode(noa_gpui_core::widgets::SelectionMode::Single)
        .height(noa_gpui_core::widgets::TABLE_HEADER + 6.0 * 38.0),
    );
    kit.tree.set_accessible_label(stack_list, "Stacks");
    let stack_list_status = status_label(kit, list_card, "click a stack to fold it");
    Box::new(TableDemo {
        table,
        search,
        status,
        pantry,
        pantry_search,
        list,
        list_status,
        tasks,
        services,
        services_status,
        containers,
        containers_status,
        stack_list,
        stack_list_status,
        collapsed,
    })
}

struct OutlineDemo {
    view: WidgetId,
    search: Option<WidgetId>,
    /// The add-folder and delete-selection buttons.
    edit: Option<(WidgetId, WidgetId)>,
    status: WidgetId,
}

impl OutlineDemo {
    /// A new folder next to the selected item (inside it, when it is a folder), named
    /// in place.
    fn new_folder(&self, cx: &mut DemoCx) -> String {
        // The new folder would not match a filter, so clear it to keep it visible while
        // it is named.
        if let Some(search) = self.search
            && cx.tree.get::<OutlineView>(self.view).unwrap().is_filtered()
        {
            cx.tree
                .update::<SearchField, _>(search, |f, cx| f.set_text(cx, ""));
            cx.tree
                .update::<OutlineView, _>(self.view, |v, cx| v.set_filter_text(cx, ""));
        }
        let view = cx.tree.get::<OutlineView>(self.view).unwrap();
        let (parent, index) = match view.lead_item() {
            Some(lead) if !view.children(lead).is_empty() => {
                (Some(lead), view.children(lead).len())
            }
            Some(lead) => {
                let parent = view.parent(lead);
                let siblings: Vec<usize> = match parent {
                    Some(p) => view.children(p).to_vec(),
                    None => (0..view.item_count())
                        .filter(|&id| view.parent(id).is_none())
                        .collect(),
                };
                let at = siblings.iter().position(|&c| c == lead);
                (parent, at.map_or(usize::MAX, |at| at + 1))
            }
            None => (None, usize::MAX),
        };
        let place = parent.map_or("the top level".to_owned(), |p| view.title(p).to_owned());
        cx.tree.update::<OutlineView, _>(self.view, |v, cx| {
            let id = v.insert_item(cx, parent, index, OutlineItem::new("untitled folder"));
            v.select_item(cx, Some(id));
            v.rename_item(cx, id);
        });
        cx.tree.set_focus(Some(self.view));
        format!("new folder in {place}: type its name, Return keeps it")
    }
}

impl Demo for OutlineDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some((add, delete)) = self.edit
            && (message.source == add || message.source == delete)
        {
            if message.downcast_ref::<ButtonClicked>().is_none() {
                return true;
            }
            let text = if message.source == add {
                self.new_folder(cx)
            } else {
                let selected = cx
                    .tree
                    .get::<OutlineView>(self.view)
                    .unwrap()
                    .selected_items();
                cx.tree
                    .update::<OutlineView, _>(self.view, |v, cx| v.remove_items(cx, &selected));
                match selected.len() {
                    0 => "select items to delete".to_owned(),
                    1 => "deleted 1 item".to_owned(),
                    n => format!("deleted {n} items"),
                }
            };
            set_status(cx, self.status, text);
            return true;
        }
        if Some(message.source) == self.search {
            if let Some(TextChanged(query)) = message.downcast_ref() {
                cx.tree
                    .update::<OutlineView, _>(self.view, |v, cx| v.set_filter_text(cx, query));
                let view = cx.tree.get::<OutlineView>(self.view).unwrap();
                let text = if view.is_filtered() {
                    format!("{} items match", view.matching_item_count())
                } else {
                    "filter cleared".to_owned()
                };
                set_status(cx, self.status, text);
            }
            return true;
        }
        if message.source != self.view {
            return false;
        }
        let view = cx.tree.get::<OutlineView>(self.view).unwrap();
        let text = if let Some(ItemsMoved { items, .. }) = message.downcast_ref() {
            // Ids changed, so announce where the first item went.
            let lead = view.selected_items().first().copied();
            let place = lead
                .and_then(|id| view.parent(id))
                .map_or("the top level".to_owned(), |p| view.title(p).to_owned());
            match items.len() {
                1 => format!("moved 1 item into {place}"),
                n => format!("moved {n} items into {place}"),
            }
        } else if let Some(ItemRenamed { name, .. }) = message.downcast_ref() {
            format!("renamed to {name}")
        } else if let Some(ItemExpanded { item, expanded }) = message.downcast_ref() {
            format!(
                "{} {}",
                if *expanded { "expanded" } else { "collapsed" },
                view.title(*item)
            )
        } else if let Some(RowSelectionChanged { lead, count }) = message.downcast_ref() {
            match lead {
                Some(lead) => format!("{count} selected · {}", view.title(*lead)),
                None => "nothing selected".to_owned(),
            }
        } else {
            return true;
        };
        set_status(cx, self.status, text);
        true
    }
}

fn project_tree() -> Vec<OutlineItem> {
    let files = |names: &[&str]| -> Vec<OutlineItem> {
        names.iter().map(|n| OutlineItem::new(*n)).collect()
    };
    vec![
        OutlineItem::new("noa-gpui")
            .expanded(true)
            .child(
                OutlineItem::new("crates")
                    .expanded(true)
                    .child(
                        OutlineItem::new("noa-gpui-core")
                            .child(OutlineItem::new("src").children(files(&[
                                "lib.rs",
                                "tree.rs",
                                "widget.rs",
                                "event.rs",
                            ])))
                            .child(OutlineItem::new("tests").children(files(&[
                                "data.rs",
                                "layout.rs",
                                "text.rs",
                            ]))),
                    )
                    .child(OutlineItem::new("noa-gpui-scene").children(files(&["lib.rs"])))
                    .child(OutlineItem::new("noa-gpui-render-wgpu").children(files(&[
                        "lib.rs",
                        "renderer.rs",
                        "backdrop.rs",
                    ]))),
            )
            .child(OutlineItem::new("apps").children(files(&["demo", "desktop", "web"])))
            .children(files(&["Cargo.toml", "README.md"])),
    ]
}

fn outline(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "OutlineView::new(items) · OutlineItem::new(..).child(..) · .set_filter_text(query) · \
         .reorderable(true) · .insert_item / .remove_items · ItemsMoved",
        420.0,
    );
    let tools = kit.tree.insert(Some(card), Container::new(row_style(8.0)));
    let search = kit.tree.insert(
        Some(tools),
        SearchField::new("Filter by name, e.g. lib", 240.0),
    );
    kit.tree
        .set_accessible_label(search, "Filter project files");
    let add = kit.tree.insert(Some(tools), Button::new("New Folder"));
    let delete = kit.tree.insert(Some(tools), Button::new("Delete"));
    let view = kit.tree.insert(
        Some(card),
        OutlineView::new(project_tree())
            .alternating(true)
            .reorderable(true)
            .renamable(true)
            .height(260.0),
    );
    kit.tree.set_accessible_label(view, "Project files");
    kit.tree.set_autosave_name(view, "catalog.project-files");
    let status = status_label(kit, card, "nothing selected");
    Box::new(OutlineDemo {
        view,
        search: Some(search),
        edit: Some((add, delete)),
        status,
    })
}

fn sidebar(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "OutlineView::new(items).sidebar() · OutlineItem::new(..).icon(..).badge(..) · \
         .reorderable(true)",
        420.0,
    );
    let icons: [ImageId; 4] = kit.assets.icons;
    let items = vec![
        OutlineItem::new("FAVORITES")
            .child(OutlineItem::new("Recents").icon(icons[1]))
            .child(OutlineItem::new("Desktop").icon(icons[3]))
            .child(OutlineItem::new("Inbox").icon(icons[0]).badge("12"))
            .child(OutlineItem::new("Drafts").icon(icons[2]).badge("3")),
        OutlineItem::new("LOCATIONS")
            .child(OutlineItem::new("Macintosh HD").icon(icons[3]))
            .child(
                OutlineItem::new("Projects")
                    .icon(icons[1])
                    .child(OutlineItem::new("noa-gpui"))
                    .child(OutlineItem::new("notes")),
            ),
        OutlineItem::new("TAGS")
            .child(OutlineItem::new("Red").icon(icons[0]))
            .child(OutlineItem::new("Blue").icon(icons[1]).badge("128")),
    ];
    let row = kit.tree.insert(
        Some(card),
        Container::new(Style {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::START,
            gap: gap(12.0, 0.0),
            ..Default::default()
        }),
    );
    let view = kit.tree.insert(
        Some(row),
        OutlineView::new(items)
            .sidebar()
            .reorderable(true)
            .width(220.0)
            .height(300.0),
    );
    kit.tree.set_accessible_label(view, "Sidebar");
    let status = kit
        .tree
        .insert(Some(row), Label::new("Select an item").color(Role::Muted));
    Box::new(OutlineDemo {
        view,
        search: None,
        edit: None,
        status,
    })
}

struct CollectionDemo {
    view: WidgetId,
    status: WidgetId,
}

impl Demo for CollectionDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.view {
            return false;
        }
        // Items are numbered by position, which a drag changes, so name them by title.
        let view = cx.tree.get::<CollectionView>(self.view).unwrap();
        let title = |i: usize| view.items()[i].title().to_owned();
        let text = if let Some(RowsMoved { rows, to }) = message.downcast_ref() {
            match rows.len() {
                1 => format!("moved {} to place {}", title(*to), to + 1),
                n => format!("moved {n} swatches to place {}", to + 1),
            }
        } else if let Some(RowSelectionChanged { count, lead }) = message.downcast_ref() {
            match lead {
                Some(lead) if *count == 1 => format!("{} selected", title(*lead)),
                _ => format!("{count} selected"),
            }
        } else if let Some(RowActivated(item)) = message.downcast_ref() {
            format!("opened {}", title(*item))
        } else {
            return true;
        };
        set_status(cx, self.status, text);
        true
    }
}

fn collection(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "CollectionView::new(items).item_size(72, 72) · CollectionItem::new(..).color(..) · \
         .reorderable(true)",
        720.0,
    );
    let items = (0..5_000)
        .map(|i| {
            let hue = (i as f32 * 137.5) % 360.0;
            let value = 0.55 + 0.4 * ((i / 7) % 3) as f32 / 2.0;
            CollectionItem::new(format!("Swatch {i}")).color(from_hsv(hue, 0.55, value))
        })
        .collect();
    let view = kit.tree.insert(
        Some(card),
        CollectionView::new(items)
            .item_size(72.0, 72.0)
            .reorderable(true)
            .height(320.0),
    );
    kit.tree.set_accessible_label(view, "Swatches");
    let status = status_label(kit, card, "0 selected");
    Box::new(CollectionDemo { view, status })
}

struct BrowserDemo {
    view: WidgetId,
    status: WidgetId,
}

impl Demo for BrowserDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.view {
            return false;
        }
        let view = cx.tree.get::<BrowserView>(self.view).unwrap();
        let text = if let Some(BrowserPathChanged(path)) = message.downcast_ref() {
            let names: Vec<&str> = path.iter().map(|&id| view.title(id)).collect();
            format!("/{}", names.join("/"))
        } else if let Some(RowActivated(item)) = message.downcast_ref() {
            format!("opened {}", view.title(*item))
        } else {
            return true;
        };
        set_status(cx, self.status, text);
        true
    }
}

fn browser(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card("BrowserView::new(items).column_width(180)", 720.0);
    let folder = |name: &str, children: Vec<OutlineItem>| {
        OutlineItem::new(name)
            .icon(kit.assets.icons[3])
            .children(children)
    };
    let files = |prefix: &str, n: usize| -> Vec<OutlineItem> {
        (1..=n)
            .map(|i| OutlineItem::new(format!("{prefix} {i:02}.txt")))
            .collect()
    };
    let items = vec![
        folder(
            "Applications",
            vec![
                OutlineItem::new("Calculator"),
                OutlineItem::new("Mail"),
                OutlineItem::new("Notes"),
            ],
        ),
        folder(
            "Documents",
            vec![
                folder("Invoices", files("Invoice", 24)),
                folder(
                    "Projects",
                    vec![
                        folder("noa-gpui", files("Spec", 6)),
                        folder("Website", files("Page", 9)),
                    ],
                ),
                folder("Letters", files("Letter", 4)),
            ],
        ),
        folder("Downloads", files("Download", 40)),
        folder(
            "Pictures",
            vec![
                folder("2025", files("Photo", 30)),
                folder("2026", files("Photo", 12)),
            ],
        ),
    ];
    let view = kit.tree.insert(
        Some(card),
        BrowserView::new(items).column_width(180.0).height(240.0),
    );
    kit.tree.set_accessible_label(view, "Folders");
    let status = status_label(kit, card, "/");
    Box::new(BrowserDemo { view, status })
}

struct RuleDemo {
    editor: WidgetId,
    table: WidgetId,
    status: WidgetId,
}

impl RuleDemo {
    /// Filters the table in place with a copy of the rules, keeping the sort, the
    /// selection of rows that still match, and the scroll position.
    fn filter(&self, cx: &mut DemoCx) {
        let matcher = cx.tree.get::<RuleEditor>(self.editor).unwrap().matcher();
        cx.tree.update::<TableView, _>(self.table, |t, cx| {
            t.set_filter(cx, move |data, row| {
                matcher.matches(|field| data.cell(row, field))
            })
        });
        let table = cx.tree.get::<TableView>(self.table).unwrap();
        let text = format!(
            "{} of {} files match",
            table.matching_row_count(),
            table.total_row_count()
        );
        set_status(cx, self.status, text);
    }
}

impl Demo for RuleDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.editor {
            if message.downcast_ref::<RulesChanged>().is_some() {
                self.filter(cx);
            }
            return true;
        }
        false
    }
}

fn rules(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "RuleEditor::new(fields, width) · RuleField::text(..) / ::number(..)",
        720.0,
    );
    let editor = kit.tree.insert(
        Some(card),
        RuleEditor::new(
            vec![
                RuleField::text("Name"),
                RuleField::number("Size"),
                RuleField::text("Kind"),
            ],
            684.0,
        ),
    );
    let rows: Vec<Vec<String>> = (0..500)
        .map(|r| {
            vec![
                format!("File {r:06}"),
                format!("{} KB", Files::size(r)),
                KINDS[r % KINDS.len()].to_owned(),
            ]
        })
        .collect();
    let table = kit.tree.insert(
        Some(card),
        TableView::new(
            vec![
                TableColumn::new("Name", 220.0).sort(ColumnSort::Text),
                TableColumn::new("Size", 140.0)
                    .sort(ColumnSort::Number)
                    .align(ColumnAlign::Trailing),
                TableColumn::new("Kind", 160.0).sort(ColumnSort::Text),
            ],
            rows,
        )
        .height(220.0),
    );
    kit.tree.set_accessible_label(table, "Matching files");
    let status = status_label(kit, card, "500 of 500 files match");
    Box::new(RuleDemo {
        editor,
        table,
        status,
    })
}

const MONTHS: [&str; 6] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
const REGIONS: [&str; 8] = [
    "North", "South", "East", "West", "Central", "Coast", "Hills", "Islands",
];

struct GridDemo {
    grid: WidgetId,
    chart: WidgetId,
    status: WidgetId,
    /// Export CSV… and Import CSV… buttons; desktop only, since they open file panels.
    files: Option<(WidgetId, WidgetId)>,
    /// A file panel is open: `true` to save the CSV, `false` to open one.
    #[cfg(feature = "native")]
    choosing: Option<bool>,
    /// The grid's data, kept in step with its edits for the chart.
    rows: Vec<Vec<String>>,
    region: usize,
}

/// The grid's rows from CSV `text` saved by Export CSV… or a spreadsheet. A header row
/// (Region, then the months) is skipped, short rows are padded with empty cells, and
/// long ones are cut to the columns. The error says why there is nothing to show.
#[cfg(feature = "native")]
fn grid_rows_from_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = noa_gpui_core::widgets::TableText::Csv.read(text);
    if rows
        .first()
        .and_then(|r| r.first())
        .is_some_and(|c| c.trim().eq_ignore_ascii_case("region"))
    {
        rows.remove(0);
    }
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    if rows.is_empty() {
        return Err("the file has no rows".to_owned());
    }
    for row in &mut rows {
        row.resize(MONTHS.len() + 1, String::new());
    }
    Ok(rows)
}

impl GridDemo {
    fn region_values(rows: &[Vec<String>], region: usize) -> Vec<f64> {
        rows[region][1..]
            .iter()
            .map(|v| v.trim().replace(',', "").parse().unwrap_or(f64::NAN))
            .collect()
    }

    fn show_region(&mut self, cx: &mut DemoCx, region: usize) {
        self.region = region;
        let name = self.rows[region][0].clone();
        let series = ChartSeries::new(name, Self::region_values(&self.rows, region));
        let labels = MONTHS.iter().map(|m| (*m).to_owned()).collect();
        cx.tree
            .update::<Chart, _>(self.chart, |c, cx| c.set_data(cx, labels, vec![series]));
    }
}

impl Demo for GridDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some((export, import)) = self.files
            && (message.source == export || message.source == import)
        {
            #[cfg(feature = "native")]
            if message.downcast_ref::<ButtonClicked>().is_some() {
                let save = message.source == export;
                let csv = vec![("CSV".to_owned(), vec!["csv".to_owned()])];
                cx.file_dialogs.push(noa_gpui_platform::FileDialogRequest {
                    save,
                    title: if save { "Export CSV" } else { "Import CSV" }.into(),
                    file_name: if save { "figures.csv" } else { "" }.into(),
                    filters: csv,
                });
                self.choosing = Some(save);
            }
            return true;
        }
        if message.source == self.chart {
            if let Some(ChartPointActivated(month)) = message.downcast_ref() {
                let (grid, region) = (self.grid, self.region);
                cx.tree
                    .update::<TableView, _>(grid, |t, cx| t.select_cell(cx, region, month + 1));
                cx.tree.set_focus(Some(grid));
                let text = format!("{} · {}", self.rows[region][0], MONTHS[*month]);
                set_status(cx, self.status, text);
            }
            return true;
        }
        if message.source != self.grid {
            return false;
        }
        let column = |c: usize| {
            if c == 0 { "Region" } else { MONTHS[c - 1] }
        };
        if let Some(CellSelectionChanged { focus, range }) = message.downcast_ref() {
            let text = if range.len() > 1 {
                format!(
                    "{} cells selected ({} rows × {} columns)",
                    range.len(),
                    range.rows.len(),
                    range.columns.len()
                )
            } else {
                format!("{} · {}", self.rows[focus.0][0], column(focus.1))
            };
            set_status(cx, self.status, text);
            if focus.0 != self.region {
                self.show_region(cx, focus.0);
            }
        }
        if let Some(CellEdited {
            row,
            column: c,
            text,
            ..
        }) = message.downcast_ref()
        {
            self.rows[*row][*c].clone_from(text);
            let status = format!("{} · {} set to {text}", self.rows[*row][0], column(*c));
            set_status(cx, self.status, status);
            if *row == self.region {
                let values = Self::region_values(&self.rows, *row);
                cx.tree
                    .update::<Chart, _>(self.chart, |ch, cx| ch.set_values(cx, 0, values));
            }
        }
        true
    }

    #[cfg(feature = "native")]
    fn file_dialog_finished(
        &mut self,
        cx: &mut DemoCx,
        result: &noa_gpui_platform::FileDialogResult,
    ) {
        use noa_gpui_platform::FileDialogResult;
        let Some(save) = self.choosing.take() else {
            return;
        };
        let text = match result {
            FileDialogResult::Saved(path) if save => {
                let grid = cx.tree.get::<TableView>(self.grid).unwrap();
                let csv = grid.export_text(noa_gpui_core::widgets::TableText::Csv);
                match std::fs::write(path, csv + "\n") {
                    Ok(()) => format!("exported {} rows to {}", self.rows.len(), path.display()),
                    Err(e) => format!("could not export: {e}"),
                }
            }
            FileDialogResult::Opened(paths) if !save => {
                let Some(path) = paths.first() else { return };
                let rows = std::fs::read_to_string(path)
                    .map_err(|e| e.to_string())
                    .and_then(|text| grid_rows_from_csv(&text));
                match rows {
                    Ok(rows) => {
                        let n = rows.len();
                        self.rows = rows;
                        let data = self.rows.clone();
                        cx.tree.update::<TableView, _>(self.grid, |t, cx| {
                            t.set_data(cx, data);
                            t.clear_undo_history();
                        });
                        self.show_region(cx, 0);
                        format!("imported {n} regions from {}", path.display())
                    }
                    Err(e) => format!("could not import {}: {e}", path.display()),
                }
            }
            _ => "cancelled".to_owned(),
        };
        set_status(cx, self.status, text);
    }
}

fn data_grid(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        ".cell_selection(true) · .frozen_columns(1) · TableColumn::editable(true) · Chart · \
         .export_text(TableText::Csv)",
        520.0,
    );
    let mut columns = vec![TableColumn::new("Region", 120.0)];
    columns.extend(MONTHS.iter().map(|m| {
        TableColumn::new(*m, 90.0)
            .align(ColumnAlign::Trailing)
            .editable(true)
    }));
    let rows: Vec<Vec<String>> = REGIONS
        .iter()
        .enumerate()
        .map(|(r, region)| {
            let mut row = vec![(*region).to_owned()];
            row.extend((0..MONTHS.len()).map(|m| format!("{}", 120 + (r * 37 + m * 53) % 400)));
            row
        })
        .collect();
    let grid = kit.tree.insert(
        Some(card),
        TableView::new(columns, rows.clone())
            .cell_selection(true)
            .frozen_columns(1)
            .height(230.0),
    );
    kit.tree.set_accessible_label(grid, "Figures by region");
    let chart = kit.tree.insert(
        Some(card),
        Chart::bar(MONTHS.iter().map(|m| (*m).to_owned()).collect())
            .series(ChartSeries::new(
                REGIONS[0],
                GridDemo::region_values(&rows, 0),
            ))
            .height(150.0),
    );
    kit.tree
        .set_accessible_label(chart, "Selected region by month");
    // Saving and opening files needs the desktop's file panels.
    let files = cfg!(feature = "native").then(|| {
        let buttons = kit.tree.insert(Some(card), Container::new(row_style(8.0)));
        let export = kit.tree.insert(Some(buttons), Button::new("Export CSV…"));
        let import = kit.tree.insert(Some(buttons), Button::new("Import CSV…"));
        (export, import)
    });
    let status = status_label(kit, card, "Click a cell");
    Box::new(GridDemo {
        grid,
        chart,
        status,
        files,
        #[cfg(feature = "native")]
        choosing: None,
        rows,
        region: 0,
    })
}

struct ChartsDemo {
    trend: WidgetId,
    live: WidgetId,
    sample: WidgetId,
    status: WidgetId,
    samples: u32,
}

impl ChartsDemo {
    /// A made-up load reading that wanders but stays in 0..100.
    fn reading(n: u32) -> [f64; 2] {
        let t = f64::from(n);
        let cpu = 45.0 + 30.0 * (t * 0.45).sin() + 12.0 * (t * 1.7).cos();
        let memory = 55.0 + 10.0 * (t * 0.12).sin();
        [cpu.clamp(0.0, 100.0), memory]
    }
}

impl Demo for ChartsDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.sample && message.downcast_ref::<ButtonClicked>().is_some() {
            for _ in 0..5 {
                self.samples += 1;
                let (n, values) = (self.samples, Self::reading(self.samples));
                cx.tree
                    .update::<Chart, _>(self.live, |c, cx| c.push(cx, format!("{n}s"), &values));
            }
            return true;
        }
        if message.source != self.trend && message.source != self.live {
            return false;
        }
        if let Some(ChartPointActivated(i)) = message.downcast_ref() {
            let chart = cx.tree.get::<Chart>(message.source);
            let text = chart
                .and_then(|c| {
                    let (label, values) = c.readout()?;
                    let values: Vec<String> = values
                        .into_iter()
                        .map(|(n, v)| format!("{n} {v}"))
                        .collect();
                    Some(format!("{label}: {}", values.join(", ")))
                })
                .unwrap_or_else(|| format!("Point {i}"));
            set_status(cx, self.status, text);
        }
        true
    }
}

fn charts(kit: &mut Kit) -> Box<dyn Demo> {
    const YEAR: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let labels: Vec<String> = YEAR.iter().map(|m| (*m).to_owned()).collect();
    let curve = |base: f64, swing: f64, phase: f64| -> Vec<f64> {
        (0..12)
            .map(|m| (base + swing * (f64::from(m) * 0.6 + phase).sin()).round())
            .collect()
    };

    let card = kit.wide_card("Chart::line(labels).series(…) · ChartSeries", 520.0);
    let trend = kit.tree.insert(
        Some(card),
        Chart::line(labels.clone())
            .series(ChartSeries::new("Visits", curve(3200.0, 1400.0, 0.0)))
            .series(ChartSeries::new("Sign-ups", curve(900.0, 500.0, 1.2)))
            .height(190.0),
    );
    kit.tree.set_accessible_label(trend, "Visits and sign-ups");

    let card = kit.wide_card("Chart::bar(labels) · .format(…)", 520.0);
    let mut revenue = curve(12.0, 9.0, 2.0);
    revenue[4] = -3.5;
    let money = kit.tree.insert(
        Some(card),
        Chart::bar(labels[..6].to_vec())
            .series(ChartSeries::new("Revenue", revenue[..6].to_vec()))
            .series(ChartSeries::new(
                "Costs",
                curve(8.0, 3.0, 0.5)[..6].to_vec(),
            ))
            .format(|v| format!("${v:.1}M"))
            .height(170.0),
    );
    kit.tree.set_accessible_label(money, "Revenue and costs");

    let card = kit.wide_card(
        ".stacked(true): parts of a whole · click the legend to leave one out",
        520.0,
    );
    let storage = kit.tree.insert(
        Some(card),
        Chart::bar(labels[..6].to_vec())
            .series(ChartSeries::new(
                "Photos",
                curve(120.0, 40.0, 0.3)[..6].to_vec(),
            ))
            .series(ChartSeries::new(
                "Documents",
                curve(60.0, 15.0, 1.1)[..6].to_vec(),
            ))
            .series(ChartSeries::new(
                "Backups",
                curve(90.0, 30.0, 2.4)[..6].to_vec(),
            ))
            .stacked(true)
            .format(|v| format!("{v:.0} GB"))
            .height(170.0),
    );
    kit.tree
        .set_accessible_label(storage, "Storage used by kind");

    let card = kit.wide_card(".range(0.0, 100.0) · .capacity(30) · push(…)", 520.0);
    let samples = 20;
    let readings: Vec<[f64; 2]> = (1..=samples).map(ChartsDemo::reading).collect();
    let live = kit.tree.insert(
        Some(card),
        Chart::line((1..=samples).map(|n| format!("{n}s")).collect())
            .series(ChartSeries::new(
                "CPU",
                readings.iter().map(|r| r[0]).collect(),
            ))
            .series(ChartSeries::new(
                "Memory",
                readings.iter().map(|r| r[1]).collect(),
            ))
            .range(0.0, 100.0)
            .capacity(30)
            .format(|v| format!("{v:.0}%"))
            .height(150.0),
    );
    kit.tree.set_accessible_label(live, "CPU and memory");
    let sample = kit.tree.insert(Some(card), Button::new("Take 5 Samples"));
    let status = status_label(kit, card, "Click a point on a chart");

    let row = kit.tree.insert(Some(card), Container::new(row_style(16.0)));
    for (name, values) in [
        (
            "Latency",
            vec![42.0, 38.0, 51.0, 47.0, 60.0, 44.0, 39.0, 35.0],
        ),
        ("Errors", vec![0.0, 2.0, 1.0, 5.0, 3.0, 0.0, 1.0, 0.0]),
    ] {
        kit.tree.insert(Some(row), Label::new(name).font_size(12.0));
        let n = values.len();
        let sparkline = kit.tree.insert(
            Some(row),
            Chart::line(vec![String::new(); n])
                .series(ChartSeries::new(name, values))
                .sparkline()
                .width(90.0)
                .height(22.0),
        );
        kit.tree.set_accessible_label(sparkline, name);
    }
    Box::new(ChartsDemo {
        trend,
        live,
        sample,
        status,
        samples,
    })
}

const SHAPES: [&str; 3] = ["Rectangle", "Ellipse", "Star"];

struct InspectorDemo {
    properties: PropertyGrid,
    summary: DescriptionListParts,
}

impl InspectorDemo {
    fn describe(value: &PropertyValue) -> String {
        match value {
            PropertyValue::Number(n) => format!("{n}"),
            PropertyValue::Color(c) => hex_string(*c),
            PropertyValue::Bool(on) => if *on { "Yes" } else { "No" }.to_owned(),
            PropertyValue::Choice(i) => SHAPES.get(*i).copied().unwrap_or("").to_owned(),
        }
    }
}

impl Demo for InspectorDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        let Some(change) = self.properties.handle(message) else {
            return false;
        };
        let text = Self::describe(&change.value);
        self.summary.set_value(cx.tree, change.index, text);
        true
    }
}

fn inspector(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card("PropertyGrid::new(tree, parent, Some(90.0)) · add_number / add_color / add_bool / add_choice", 300.0);
    let mut properties = PropertyGrid::new(kit.tree, card, Some(90.0));
    properties.add_section(kit.tree, "GEOMETRY");
    properties.add_number_with(
        kit.tree,
        "Width",
        NumberInput::new(0.0, 2000.0, 320.0).unit("px").width(110.0),
    );
    properties.add_number_with(
        kit.tree,
        "Corner",
        NumberInput::new(0.0, 64.0, 8.0).unit("px").width(110.0),
    );
    properties.add_choice(kit.tree, "Shape", SHAPES, 0);
    properties.add_section(kit.tree, "APPEARANCE");
    properties.add_color(kit.tree, "Fill", Color::hex(0x3d7be0));
    properties.add_bool(kit.tree, "Shadow", true);
    for i in 0..properties.len() {
        if let (Some(editor), Some(name)) = (properties.editor(i), properties.name(i)) {
            kit.tree.set_accessible_label(editor, name);
        }
    }

    let card = kit.wide_card(
        "DescriptionList::new().item(label, value).insert(tree, parent)",
        260.0,
    );
    let items: Vec<(String, String)> = (0..properties.len())
        .map(|i| {
            let name = properties.name(i).unwrap_or_default().to_owned();
            let value = properties
                .value(i)
                .map(InspectorDemo::describe)
                .unwrap_or_default();
            (name, value)
        })
        .collect();
    let summary = DescriptionList::new().items(items).insert(kit.tree, card);
    let card = kit.wide_card(
        "DescriptionList::new().separators(true).value_font_family(\"monospace\")",
        260.0,
    );
    DescriptionList::new()
        .separators(true)
        .value_font_family("monospace")
        .value_font_size(12.0)
        .item("Target", "unix:///var/run/docker.sock")
        .item("Engine", "Docker Engine 27.1 · API 1.46 · linux/arm64")
        .item("Compose", "v2.29.1")
        .insert(kit.tree, card);
    Box::new(InspectorDemo {
        properties,
        summary,
    })
}

const LOG_LINES: [&str; 6] = [
    "connected to build server",
    "resolving dependencies",
    "compiling noa-gpui-core",
    "compiling demo",
    "running 412 tests",
    "all tests passed",
];

struct FollowDemo {
    view: WidgetId,
    lines: WidgetId,
    add: WidgetId,
    jump: WidgetId,
    status: WidgetId,
    count: usize,
    following: bool,
}

impl FollowDemo {
    fn add_lines(&mut self, tree: &mut UiTree, n: usize) {
        for _ in 0..n {
            let line = format!(
                "{:04}  {}",
                self.count + 1,
                LOG_LINES[self.count % LOG_LINES.len()]
            );
            tree.insert(Some(self.lines), Label::new(line).font_size(12.0));
            self.count += 1;
        }
    }
}

impl Demo for FollowDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        if message.source == self.add {
            self.add_lines(cx.tree, 3);
            return true;
        }
        if message.source == self.jump {
            cx.tree
                .update::<ScrollView, _>(self.view, |v, cx| v.scroll_to_end(cx));
            return true;
        }
        false
    }

    fn after_input(&mut self, tree: &mut UiTree) {
        let following = tree
            .get::<ScrollView>(self.view)
            .is_some_and(ScrollView::is_following_end);
        if following != self.following {
            self.following = following;
            let text = if following {
                "Following the end"
            } else {
                "Scrolled up: new lines do not move the view"
            };
            tree.update::<Label, _>(self.status, |l, cx| l.set_text(cx, text));
        }
    }
}

fn follow_end(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card("ScrollView::new(style).follows_end(true)", 360.0);
    let view = kit.tree.insert(
        Some(card),
        ScrollView::new(Style {
            size: LayoutSize {
                width: percent(1.0),
                height: length(160.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        })
        .follows_end(true),
    );
    let lines = kit.tree.insert(
        Some(view),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 2.0),
            size: LayoutSize {
                width: percent(1.0),
                height: auto(),
            },
            padding: noa_gpui_core::style::Rect::length(8.0),
            flex_shrink: 0.0,
            ..Default::default()
        })
        .background(Role::SurfaceAlt),
    );
    let buttons = kit.tree.insert(Some(card), Container::new(row_style(8.0)));
    let add = kit.tree.insert(Some(buttons), Button::new("Add Lines"));
    let jump = kit.tree.insert(Some(buttons), Button::new("Jump to End"));
    let status = status_label(kit, card, "Following the end");
    let mut demo = FollowDemo {
        view,
        lines,
        add,
        jump,
        status,
        count: 0,
        following: true,
    };
    demo.add_lines(kit.tree, 12);
    Box::new(demo)
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::grid_rows_from_csv;

    #[test]
    fn exported_csv_comes_back_as_the_grids_rows() {
        let csv = "Region,Jan,Feb,Mar,Apr,May,Jun\n\"North, east\",1,2,3,4,5,6\nSouth,7\n,,\n";
        let rows = grid_rows_from_csv(csv).unwrap();
        assert_eq!(rows.len(), 2, "the header and the empty row are skipped");
        assert_eq!(rows[0][0], "North, east");
        assert_eq!(rows[1], ["South", "7", "", "", "", "", ""]);
        assert!(grid_rows_from_csv("Region,Jan\n").is_err());
        assert!(grid_rows_from_csv("").is_err());
    }
}

const ORDERS: [(&str, &str, &str, &str); 9] = [
    ("Hardware", "Keyboard", "12", "1,080"),
    ("Software", "IDE license", "3", "1,497"),
    ("Hardware", "Monitor", "4", "1,596"),
    ("Services", "Support plan", "1", "2,400"),
    ("Software", "Font pack", "10", "290"),
    ("Hardware", "Dock", "6", "714"),
    ("Services", "Onboarding", "2", "1,800"),
    ("Software", "Backup", "5", "250"),
    ("Hardware", "Webcam", "8", "632"),
];

fn orders() -> Vec<Vec<String>> {
    ORDERS
        .iter()
        .map(|(a, b, c, d)| [a, b, c, d].iter().map(|s| (**s).to_owned()).collect())
        .collect()
}

struct GroupingDemo {
    table: WidgetId,
    grouping: Grouping,
}

impl Demo for GroupingDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.table {
            return false;
        }
        if let Some(GroupRowClicked(row)) = message.downcast_ref()
            && self.grouping.toggle_at(&orders(), *row).is_some()
        {
            let data = self.grouping.apply(orders());
            cx.tree
                .update::<TableView, _>(self.table, |t, cx| t.set_data(cx, data));
        }
        true
    }
}

fn grouping(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "Grouping::by(0).aggregate(2, Aggregate::Sum).aggregate(3, Aggregate::Sum) \
         · TableView::group_row_cells(true)",
        560.0,
    );
    let grouping = Grouping::by(0)
        .aggregate(2, Aggregate::Sum)
        .aggregate(3, Aggregate::Sum);
    let columns = vec![
        TableColumn::new("Category", 140.0),
        TableColumn::new("Item", 160.0).sort(ColumnSort::Text),
        TableColumn::new("Qty", 80.0)
            .align(ColumnAlign::Trailing)
            .sort(ColumnSort::Number),
        TableColumn::new("Amount", 120.0)
            .align(ColumnAlign::Trailing)
            .sort(ColumnSort::Number),
    ];
    let table = kit.tree.insert(
        Some(card),
        TableView::new(columns, grouping.apply(orders()))
            .group_row_cells(true)
            .height(320.0),
    );
    kit.tree.set_accessible_label(table, "Orders by category");
    Box::new(GroupingDemo { table, grouping })
}

const REMOTE_ROWS: usize = 1_000_000;
const REMOTE_PAGE: usize = 50;
/// Orders by amount are orders by id times this, modulo the row count (coprime with
/// it), so either order is computed without sorting a million rows.
const AMOUNT_STEP: usize = 7_919;
const CUSTOMERS: [&str; 6] = ["Aoki", "Baker", "Chen", "Diaz", "Endo", "Fischer"];

/// The simulated server: the rows at `rows` sorted by `sort` (column, ascending).
fn remote_rows(rows: std::ops::Range<usize>, sort: Option<(usize, bool)>) -> Vec<Vec<String>> {
    // The inverse of AMOUNT_STEP modulo REMOTE_ROWS, so an id's amount rank is known.
    const INVERSE: usize = {
        let (mut t, mut new_t, mut r, mut new_r) =
            (0i64, 1i64, REMOTE_ROWS as i64, AMOUNT_STEP as i64);
        while new_r != 0 {
            let q = r / new_r;
            (t, new_t) = (new_t, t - q * new_t);
            (r, new_r) = (new_r, r - q * new_r);
        }
        (t.rem_euclid(REMOTE_ROWS as i64)) as usize
    };
    rows.map(|i| {
        let (column, ascending) = sort.unwrap_or((0, true));
        let at = if ascending { i } else { REMOTE_ROWS - 1 - i };
        let (id, rank) = if column == 2 {
            (at * AMOUNT_STEP % REMOTE_ROWS, at)
        } else {
            (at, at * INVERSE % REMOTE_ROWS)
        };
        vec![
            format!("#{id:07}"),
            CUSTOMERS[id % CUSTOMERS.len()].to_owned(),
            format!("{:.2}", 1.0 + rank as f64 / 100.0),
        ]
    })
    .collect()
}

type Delivery = (PageRequest, Vec<Vec<String>>);

struct RemoteDemo {
    table: WidgetId,
    status: WidgetId,
    rows: RemoteRows,
    sort: Option<(usize, bool)>,
    waker: Option<noa_gpui_platform::Waker>,
    sender: std::sync::mpsc::Sender<Delivery>,
    arrived: std::sync::mpsc::Receiver<Delivery>,
}

impl RemoteDemo {
    fn fetch(&self, request: PageRequest) {
        let (sender, sort) = (self.sender.clone(), self.sort);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let waker = self.waker.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                let rows = remote_rows(request.rows.clone(), sort);
                let _ = sender.send((request, rows));
                if let Some(waker) = waker {
                    waker.wake();
                }
            });
        }
        // No threads on the Web: the "server" answers at once.
        #[cfg(target_arch = "wasm32")]
        {
            let rows = remote_rows(request.rows.clone(), sort);
            let _ = sender.send((request, rows));
            if let Some(waker) = &self.waker {
                waker.wake();
            }
        }
    }

    fn show_status(&self, cx: &mut DemoCx) {
        let text = format!(
            "{} of {} pages loaded",
            self.rows.loaded_pages(),
            REMOTE_ROWS / REMOTE_PAGE
        );
        set_status(cx, self.status, text);
    }
}

impl Demo for RemoteDemo {
    fn woken(&mut self, cx: &mut DemoCx) {
        let mut changed = false;
        while let Ok((request, rows)) = self.arrived.try_recv() {
            changed |= self.rows.fulfill_request(&request, rows);
        }
        for request in self.rows.take_requests() {
            self.fetch(request);
        }
        if changed {
            cx.tree
                .update::<TableView, _>(self.table, |t, cx| t.data_changed(cx));
            self.show_status(cx);
        }
    }

    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.table {
            return false;
        }
        if let Some(&SortChanged { column, ascending }) = message.downcast_ref() {
            self.sort = Some((column, ascending));
            self.rows.reset(REMOTE_ROWS);
            cx.tree.update::<TableView, _>(self.table, |t, cx| {
                t.data_changed(cx);
                t.scroll_to_row(cx, 0);
            });
            self.show_status(cx);
        }
        true
    }
}

fn remote(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "RemoteRows::new(1_000_000, 50) · TableView::new(columns, rows.data()).sorts_rows(false)",
        560.0,
    );
    let rows = RemoteRows::new(REMOTE_ROWS, REMOTE_PAGE);
    if let Some(waker) = kit.waker.clone() {
        rows.on_request(move || waker.wake());
    }
    let columns = vec![
        TableColumn::new("Order", 160.0).sort(ColumnSort::Text),
        TableColumn::new("Customer", 180.0),
        TableColumn::new("Amount", 140.0)
            .align(ColumnAlign::Trailing)
            .sort(ColumnSort::Number),
    ];
    let table = kit.tree.insert(
        Some(card),
        TableView::new(columns, rows.data())
            .sorts_rows(false)
            .height(320.0),
    );
    kit.tree.set_accessible_label(table, "Orders on the server");
    let status = status_label(kit, card, "0 pages loaded");
    let (sender, arrived) = std::sync::mpsc::channel();
    Box::new(RemoteDemo {
        table,
        status,
        rows,
        sort: None,
        waker: kit.waker.clone(),
        sender,
        arrived,
    })
}

#[cfg(test)]
mod remote_tests {
    use super::*;

    #[test]
    fn the_simulated_server_returns_every_order_once_in_either_order() {
        let by_amount = remote_rows(0..REMOTE_ROWS, Some((2, true)));
        let mut ids: Vec<&str> = by_amount.iter().map(|r| r[0].as_str()).collect();
        let amounts: Vec<f64> = by_amount.iter().map(|r| r[2].parse().unwrap()).collect();
        assert!(amounts.windows(2).all(|w| w[0] < w[1]));
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), REMOTE_ROWS);
        // An order has the same amount whichever way the rows are sorted.
        let by_id = remote_rows(0..REMOTE_ROWS, None);
        let row = &by_amount[12_345];
        let id: usize = row[0][1..].parse().unwrap();
        assert_eq!(by_id[id], *row);
        assert_eq!(remote_rows(0..1, Some((0, false)))[0][0], "#0999999");
    }
}
