//! Rows that live elsewhere, such as on a server, loaded a page at a time as the
//! table shows them.
//!
//! [`RemoteRows`] is a handle the app keeps; [`RemoteRows::data`] is what a
//! [`super::TableView`] shows. When the table draws a row whose page is not loaded,
//! the row shows [`super::CellLook::Loading`] and the page is asked for:
//! [`RemoteRows::on_request`] is called (wake the event loop there), and
//! [`RemoteRows::take_requests`] hands out the pages to fetch. When a page arrives,
//! give it to [`RemoteRows::fulfill`] and call [`super::TableView::data_changed`].
//!
//! ```
//! # use noa_gpui_core::widgets::{RemoteRows, TableColumn, TableView};
//! let rows = RemoteRows::new(1_000_000, 100);
//! let table = TableView::new(vec![TableColumn::new("Name", 200.0)], rows.data())
//!     .sorts_rows(false);
//! // Later, on the main thread, once the table has drawn:
//! for request in rows.take_requests() {
//!     // Fetch `request.rows` in the background; when they arrive:
//!     let fetched = request.rows.clone().map(|r| vec![format!("Row {r}")]).collect();
//!     rows.fulfill(request.page, fetched);
//! }
//! ```
//!
//! Sorting and filtering read every row, so leave them to the source: turn the
//! table's own sorting off ([`super::TableView::sorts_rows`]), and on
//! [`super::SortChanged`] or a new search ask the source again and call
//! [`RemoteRows::reset`].

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ops::Range;
use std::rc::Rc;

use super::table::{CellLook, TableData};

/// A page [`RemoteRows`] wants fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRequest {
    pub page: usize,
    /// The rows of the page, clipped to the row count.
    pub rows: Range<usize>,
    /// Counts the [`RemoteRows::reset`]s: a page fetched for an older generation is
    /// refused by [`RemoteRows::fulfill_generation`].
    pub generation: u64,
}

/// The pages kept at most, by default (older pages are dropped and fetched again
/// when they are shown).
const DEFAULT_CACHED_PAGES: usize = 64;

struct State {
    total: usize,
    page_size: usize,
    pages: HashMap<usize, Vec<Vec<String>>>,
    /// Loaded pages, least recently loaded first.
    loaded: VecDeque<usize>,
    max_pages: usize,
    /// Asked for and not fulfilled (or failed) yet.
    requested: BTreeSet<usize>,
    /// Asked for and not handed out by `take_requests` yet.
    pending: Vec<usize>,
    generation: u64,
    notify: Option<Rc<dyn Fn()>>,
}

impl State {
    fn page_rows(&self, page: usize) -> Range<usize> {
        let start = page * self.page_size;
        start.min(self.total)..(start + self.page_size).min(self.total)
    }
}

/// The app's handle on rows loaded a page at a time (see the module documentation).
/// Clones share the rows.
#[derive(Clone)]
pub struct RemoteRows {
    state: Rc<RefCell<State>>,
}

impl std::fmt::Debug for RemoteRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.state.borrow();
        f.debug_struct("RemoteRows")
            .field("total", &s.total)
            .field("page_size", &s.page_size)
            .field("loaded_pages", &s.pages.len())
            .field("requested", &s.requested.len())
            .finish()
    }
}

impl RemoteRows {
    /// `total` rows in pages of `page_size` (at least 1).
    pub fn new(total: usize, page_size: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                total,
                page_size: page_size.max(1),
                pages: HashMap::new(),
                loaded: VecDeque::new(),
                max_pages: DEFAULT_CACHED_PAGES,
                requested: BTreeSet::new(),
                pending: Vec::new(),
                generation: 0,
                notify: None,
            })),
        }
    }

    /// Keep at most `pages` pages loaded (64 by default); the page loaded longest ago
    /// is dropped first. Keep it above the pages a screen shows.
    pub fn cached_pages(self, pages: usize) -> Self {
        self.state.borrow_mut().max_pages = pages.max(1);
        self
    }

    /// Called when a page is asked for and nothing else is waiting, such as from
    /// drawing: wake the event loop here, and take the requests once it runs. Not
    /// called again until [`RemoteRows::take_requests`] took them.
    pub fn on_request(&self, notify: impl Fn() + 'static) {
        self.state.borrow_mut().notify = Some(Rc::new(notify));
    }

    /// The table data that shows these rows.
    pub fn data(&self) -> RemoteData {
        RemoteData { rows: self.clone() }
    }

    pub fn total(&self) -> usize {
        self.state.borrow().total
    }

    /// Pages asked for since the last call, to fetch.
    pub fn take_requests(&self) -> Vec<PageRequest> {
        let mut s = self.state.borrow_mut();
        let pending = std::mem::take(&mut s.pending);
        pending
            .into_iter()
            .map(|page| PageRequest {
                page,
                rows: s.page_rows(page),
                generation: s.generation,
            })
            .collect()
    }

    /// Whether pages are waiting for [`RemoteRows::take_requests`].
    pub fn has_requests(&self) -> bool {
        !self.state.borrow().pending.is_empty()
    }

    /// Store page `page`: its rows in order, each a cell per column. Rows past the
    /// page or the row count are ignored. Call [`super::TableView::data_changed`]
    /// afterwards.
    pub fn fulfill(&self, page: usize, rows: Vec<Vec<String>>) {
        let mut s = self.state.borrow_mut();
        s.requested.remove(&page);
        s.pending.retain(|&p| p != page);
        let mut rows = rows;
        rows.truncate(s.page_rows(page).len());
        if s.pages.insert(page, rows).is_none() {
            s.loaded.push_back(page);
        }
        while s.loaded.len() > s.max_pages {
            if let Some(old) = s.loaded.pop_front() {
                s.pages.remove(&old);
            }
        }
    }

    /// [`RemoteRows::fulfill`] for a page fetched for `request`: refused (returns
    /// `false`) when the rows were [reset](RemoteRows::reset) since it was asked for.
    pub fn fulfill_request(&self, request: &PageRequest, rows: Vec<Vec<String>>) -> bool {
        self.fulfill_generation(request.generation, request.page, rows)
    }

    /// [`RemoteRows::fulfill`] if `generation` is still the current one.
    pub fn fulfill_generation(&self, generation: u64, page: usize, rows: Vec<Vec<String>>) -> bool {
        if generation != self.state.borrow().generation {
            return false;
        }
        self.fulfill(page, rows);
        true
    }

    /// The page could not be fetched: it is asked for again the next time it is shown.
    pub fn fail(&self, page: usize) {
        let mut s = self.state.borrow_mut();
        s.requested.remove(&page);
        s.pending.retain(|&p| p != page);
    }

    /// Drop every page, for rows that changed at the source (another sort order or
    /// search), now `total` rows. Pages being fetched for the old rows are refused when
    /// they arrive ([`RemoteRows::fulfill_request`]). Call
    /// [`super::TableView::data_changed`] afterwards.
    pub fn reset(&self, total: usize) {
        let mut s = self.state.borrow_mut();
        s.total = total;
        s.pages.clear();
        s.loaded.clear();
        s.requested.clear();
        s.pending.clear();
        s.generation += 1;
    }

    /// Whether `row`'s page is loaded.
    pub fn is_loaded(&self, row: usize) -> bool {
        let s = self.state.borrow();
        s.pages.contains_key(&(row / s.page_size))
    }

    /// The number of pages loaded now.
    pub fn loaded_pages(&self) -> usize {
        self.state.borrow().pages.len()
    }

    /// The cell if its page is loaded; otherwise the page is asked for.
    fn cell(&self, row: usize, column: usize) -> Option<String> {
        let notify = {
            let mut s = self.state.borrow_mut();
            let page = row / s.page_size;
            if let Some(rows) = s.pages.get(&page) {
                return Some(
                    rows.get(row - page * s.page_size)
                        .and_then(|cells| cells.get(column))
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            if row >= s.total || !s.requested.insert(page) {
                return None;
            }
            let first = s.pending.is_empty();
            s.pending.push(page);
            first.then(|| s.notify.clone()).flatten()
        };
        // Outside the borrow: the callback may look at the rows.
        if let Some(notify) = notify {
            notify();
        }
        None
    }
}

/// The [`TableData`] of [`RemoteRows`]. Rows not loaded yet show as
/// [`CellLook::Loading`] and sort last.
pub struct RemoteData {
    rows: RemoteRows,
}

impl RemoteData {
    pub fn rows(&self) -> &RemoteRows {
        &self.rows
    }
}

impl TableData for RemoteData {
    fn row_count(&self) -> usize {
        self.rows.total()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        Cow::Owned(self.rows.cell(row, column).unwrap_or_default())
    }

    /// A row drawn before its page is loaded is a placeholder, and asks for the page
    /// (the table draws a placeholder without reading the cell).
    fn cell_look(&self, row: usize, column: usize) -> CellLook {
        if self.rows.is_loaded(row) {
            CellLook::Text
        } else {
            self.rows.cell(row, column);
            CellLook::Loading
        }
    }

    /// Not loaded rows have no number; sorting them asks for nothing.
    fn sort_number(&self, row: usize, column: usize) -> Option<f64> {
        if !self.rows.is_loaded(row) {
            return None;
        }
        super::table::leading_number(&self.rows.cell(row, column)?)
    }
}
