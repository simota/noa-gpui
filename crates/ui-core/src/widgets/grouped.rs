//! Grouping a table's rows by a column, with a heading row per group that counts its
//! rows and aggregates columns (sum, average, minimum, maximum), and groups that fold.
//!
//! [`Grouping`] says how; [`Grouping::apply`] wraps any [`TableData`] in a
//! [`GroupedData`] that a [`super::TableView`] shows as is: group rows are the table's
//! group rows (pinned while scrolling, rows sorted and filtered within them), and the
//! other rows are the source's, with their looks, buttons and editing.
//!
//! ```
//! # use noa_gpui_core::widgets::{Aggregate, Grouping, TableColumn, TableView};
//! let rows = vec![
//!     vec!["East".to_owned(), "Tokyo".to_owned(), "120".to_owned()],
//!     vec!["West".to_owned(), "Osaka".to_owned(), "80".to_owned()],
//!     vec!["East".to_owned(), "Sendai".to_owned(), "40".to_owned()],
//! ];
//! let grouping = Grouping::by(0).aggregate(2, Aggregate::Sum);
//! let columns = vec![
//!     TableColumn::new("Region", 120.0),
//!     TableColumn::new("City", 120.0),
//!     TableColumn::new("Sales", 80.0),
//! ];
//! let table = TableView::new(columns, grouping.apply(rows.clone())).group_row_cells(true);
//! ```
//!
//! A click on a group row emits [`super::GroupRowClicked`]; fold or unfold it with
//! [`Grouping::toggle_at`] and give the table the data again
//! ([`super::TableView::set_data_keeping_selection`] keeps the selection).

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use super::table::{CellButton, CellLook, RowSeparator, TableData};
use super::tone::Tone;

/// How a group row sums up a column over the group's rows. Cells whose text does not
/// start with a number ([`TableData::sort_number`]) are left out, except by `Count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    Count,
    Sum,
    Average,
    Min,
    Max,
}

impl Aggregate {
    fn compute(self, values: &[f64], rows: usize) -> Option<f64> {
        if self == Self::Count {
            return Some(rows as f64);
        }
        if values.is_empty() {
            return None;
        }
        let sum: f64 = values.iter().sum();
        Some(match self {
            Self::Count => unreachable!("handled above"),
            Self::Sum => sum,
            Self::Average => sum / values.len() as f64,
            Self::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
            Self::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        })
    }
}

/// Whole numbers without a fraction, others to two places at most, with thousands
/// separators: `3,400`, `283.33`.
fn format_number(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let text = if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        let text = format!("{rounded:.2}");
        text.trim_end_matches('0').to_owned()
    };
    let (sign, digits) = text
        .strip_prefix('-')
        .map_or(("", text.as_str()), |d| ("-", d));
    let (whole, fraction) = digits
        .split_once('.')
        .map_or((digits, None), |(w, f)| (w, Some(f)));
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    match fraction {
        Some(fraction) => format!("{sign}{grouped}.{fraction}"),
        None => format!("{sign}{grouped}"),
    }
}

/// Which column groups the rows, what the group rows sum up, and which groups are
/// folded. Groups come in the order their first row has in the source.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Grouping {
    column: usize,
    aggregates: Vec<(usize, Aggregate)>,
    collapsed: BTreeSet<String>,
}

impl Grouping {
    /// Group the rows by the text of `column`.
    pub fn by(column: usize) -> Self {
        Self {
            column,
            ..Self::default()
        }
    }

    /// Show `aggregate` of `column` in that column's cell of the group rows. Group rows
    /// show their cells with [`super::TableView::group_row_cells`]; without it they
    /// show only the heading and the row count.
    pub fn aggregate(mut self, column: usize, aggregate: Aggregate) -> Self {
        self.aggregates.push((column, aggregate));
        self
    }

    pub fn column(&self) -> usize {
        self.column
    }

    pub fn is_collapsed(&self, key: &str) -> bool {
        self.collapsed.contains(key)
    }

    /// Fold or unfold the group with this key. Returns whether it is folded now.
    pub fn toggle(&mut self, key: &str) -> bool {
        if self.collapsed.remove(key) {
            false
        } else {
            self.collapsed.insert(key.to_owned());
            true
        }
    }

    /// Fold or unfold the group whose group row is `row` of `source` grouped this way
    /// (the row a [`super::GroupRowClicked`] names). Returns the group's key, or
    /// `None` when `row` is not a group row.
    pub fn toggle_at(&mut self, source: &dyn TableData, row: usize) -> Option<String> {
        let layout = Layout::new(source, self);
        let Row::Group(group) = *layout.rows.get(row)? else {
            return None;
        };
        let key = layout.groups[group].key.clone();
        self.toggle(&key);
        Some(key)
    }

    /// `source` with a group row before each group.
    pub fn apply<D: TableData>(&self, source: D) -> GroupedData<D> {
        let layout = Layout::new(&source, self);
        GroupedData {
            source,
            grouping: self.clone(),
            layout,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    Group(usize),
    Item(usize),
}

#[derive(Debug)]
struct Group {
    key: String,
    /// The source rows in the group, folded or not.
    members: Vec<usize>,
    /// One per aggregate of the grouping, in its order.
    values: Vec<Option<f64>>,
}

impl Group {
    fn compute(&mut self, source: &dyn TableData, grouping: &Grouping) {
        self.values = grouping
            .aggregates
            .iter()
            .map(|&(column, aggregate)| {
                let numbers: Vec<f64> = self
                    .members
                    .iter()
                    .filter_map(|&r| source.sort_number(r, column))
                    .collect();
                aggregate.compute(&numbers, self.members.len())
            })
            .collect();
    }
}

#[derive(Debug)]
struct Layout {
    rows: Vec<Row>,
    groups: Vec<Group>,
}

impl Layout {
    fn new(source: &dyn TableData, grouping: &Grouping) -> Self {
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut members: Vec<Vec<usize>> = Vec::new();
        let mut keys = Vec::new();
        for row in 0..source.row_count() {
            // A source's own group rows have no place among the new groups.
            if source.is_group_row(row) {
                continue;
            }
            let key = source.cell(row, grouping.column).into_owned();
            let group = *index.entry(key.clone()).or_insert_with(|| {
                members.push(Vec::new());
                keys.push(key);
                members.len() - 1
            });
            members[group].push(row);
        }
        let mut rows = Vec::with_capacity(source.row_count() + keys.len());
        let mut groups = Vec::with_capacity(keys.len());
        for (g, (key, members)) in keys.into_iter().zip(members).enumerate() {
            rows.push(Row::Group(g));
            if !grouping.collapsed.contains(&key) {
                rows.extend(members.iter().map(|&r| Row::Item(r)));
            }
            let mut group = Group {
                key,
                members,
                values: Vec::new(),
            };
            group.compute(source, grouping);
            groups.push(group);
        }
        Self { rows, groups }
    }
}

/// A [`TableData`] grouped by a [`Grouping`] (see the module documentation).
pub struct GroupedData<D> {
    source: D,
    grouping: Grouping,
    layout: Layout,
}

impl<D: TableData> GroupedData<D> {
    pub fn source(&self) -> &D {
        &self.source
    }

    pub fn grouping(&self) -> &Grouping {
        &self.grouping
    }

    /// The source row shown at `row`, or `None` for a group row.
    pub fn source_row(&self, row: usize) -> Option<usize> {
        match self.layout.rows.get(row)? {
            Row::Item(r) => Some(*r),
            Row::Group(_) => None,
        }
    }

    /// The key of the group whose group row is `row`.
    pub fn group_key(&self, row: usize) -> Option<&str> {
        match self.layout.rows.get(row)? {
            Row::Group(g) => Some(&self.layout.groups[*g].key),
            Row::Item(_) => None,
        }
    }

    /// The number of groups.
    pub fn group_count(&self) -> usize {
        self.layout.groups.len()
    }

    fn group(&self, row: usize) -> Option<&Group> {
        match self.layout.rows.get(row)? {
            Row::Group(g) => Some(&self.layout.groups[*g]),
            Row::Item(_) => None,
        }
    }

    fn aggregate_text(&self, group: &Group, column: usize) -> Option<String> {
        let at = self
            .grouping
            .aggregates
            .iter()
            .position(|&(c, _)| c == column)?;
        group.values[at].map(format_number)
    }
}

impl<D: TableData> TableData for GroupedData<D> {
    fn row_count(&self) -> usize {
        self.layout.rows.len()
    }

    /// A group row shows its key (with ▾ or ▸ for unfolded or folded) in the first
    /// column, the key in the grouping column, and the aggregates in their columns.
    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        match self.layout.rows.get(row) {
            Some(Row::Item(r)) => self.source.cell(*r, column),
            Some(Row::Group(g)) => {
                let group = &self.layout.groups[*g];
                if column == 0 {
                    let mark = if self.grouping.is_collapsed(&group.key) {
                        "▸"
                    } else {
                        "▾"
                    };
                    return Cow::Owned(format!("{mark} {}", group.key));
                }
                if column == self.grouping.column {
                    return Cow::Borrowed(&group.key);
                }
                self.aggregate_text(group, column)
                    .map_or(Cow::Borrowed(""), Cow::Owned)
            }
            None => Cow::Borrowed(""),
        }
    }

    fn is_group_row(&self, row: usize) -> bool {
        matches!(self.layout.rows.get(row), Some(Row::Group(_)))
    }

    /// Edits go to the source's row, and the group's aggregates follow. Group rows take
    /// none, and neither does the grouping column: an edit there would move the row to
    /// another group, so it is left to the app (which gets [`super::CellEdited`] with
    /// `stored` false), to store it and apply the grouping again.
    fn set_cell(&mut self, row: usize, column: usize, text: &str) -> bool {
        let Some(Row::Item(r)) = self.layout.rows.get(row).copied() else {
            return false;
        };
        if column == self.grouping.column || !self.source.set_cell(r, column, text) {
            return false;
        }
        if let Some(group) = self
            .layout
            .groups
            .iter_mut()
            .find(|g| g.members.contains(&r))
        {
            group.compute(&self.source, &self.grouping);
        }
        true
    }

    fn cell_look(&self, row: usize, column: usize) -> CellLook {
        self.source_row(row)
            .map_or(CellLook::Text, |r| self.source.cell_look(r, column))
    }

    fn cell_buttons(&self, row: usize, column: usize) -> Vec<CellButton> {
        self.source_row(row)
            .map_or_else(Vec::new, |r| self.source.cell_buttons(r, column))
    }

    fn cell_detail(&self, row: usize, column: usize) -> Option<Cow<'_, str>> {
        self.source.cell_detail(self.source_row(row)?, column)
    }

    /// After a group row's heading: the number of rows (`3 rows`). The aggregates are
    /// in their own columns.
    fn cell_note(&self, row: usize, column: usize) -> Option<Cow<'_, str>> {
        let Some(group) = self.group(row) else {
            return self.source.cell_note(self.source_row(row)?, column);
        };
        if column != 0 {
            return None;
        }
        Some(Cow::Owned(if group.members.len() == 1 {
            "1 row".to_owned()
        } else {
            format!("{} rows", group.members.len())
        }))
    }

    fn cell_dot(&self, row: usize, column: usize) -> Option<Tone> {
        self.source.cell_dot(self.source_row(row)?, column)
    }

    fn row_separator(&self, row: usize) -> RowSeparator {
        self.source_row(row)
            .map_or(RowSeparator::Line, |r| self.source.row_separator(r))
    }

    fn sort_number(&self, row: usize, column: usize) -> Option<f64> {
        match self.source_row(row) {
            Some(r) => self.source.sort_number(r, column),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_formatted_with_separators_and_at_most_two_places() {
        assert_eq!(format_number(3400.0), "3,400");
        assert_eq!(format_number(1234567.891), "1,234,567.89");
        assert_eq!(format_number(283.3333), "283.33");
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(format_number(-1200.0), "-1,200");
        assert_eq!(format_number(12.0), "12");
    }
}
