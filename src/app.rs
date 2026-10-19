use crate::data::{ChartKind, Data};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Profile(usize),
    Help,
    TablePicker { tables: Vec<String>, selected: usize },
}

pub struct App {
    pub data: Data,
    pub source_path: Option<std::path::PathBuf>,
    pub sel: Cell,
    pub hcol: usize, // first visible column when horizontally scrolled
    pub chart_on: bool,
    pub chart_kind: ChartKind, // current kind for numeric cols
    pub chart_col: usize,      // col the cached chart was built for
    pub chart_valid: bool,
    pub chart_data: Option<crate::data::ChartData>,
    pub overlay: Overlay,
    pub profiles: std::collections::HashMap<usize, crate::data::Profile>,
    // search: `/` opens a prompt; Enter commits and jumps to the first hit;
    // `n` walks the hit list; all hits (row-major) are precomputed on commit.
    pub search_mode: bool,
    pub search_input: String,
    pub finding: Option<String>, // committed query (drives highlighting)
    pub hits: Vec<Cell>,
    pub hidx: usize, // index into hits of the current hit (usize::MAX = none)
    // filter (f)
    pub filter_mode: bool,
    pub filter_input: String,
    pub active_filter: Option<String>,
    // export (e)
    pub export_mode: bool,
    pub export_input: String,
    // bivariate charting (x)
    pub x_col: Option<usize>,
    // conditional formatting / heatmap (H)
    pub heatmap: bool,
    pub pinned_cols: usize,
    pub col_widths: std::collections::HashMap<usize, usize>,
    pub msg: Option<(String, &'static str)>,
    pub quit: bool,
}

impl App {
    pub fn new(data: Data) -> Self {
        Self::with_path(data, None)
    }

    pub fn with_path(data: Data, source_path: Option<std::path::PathBuf>) -> Self {
        App {
            data,
            source_path,
            sel: Cell { row: 0, col: 0 },
            hcol: 0,
            chart_on: false,
            chart_kind: ChartKind::Hist,
            chart_col: usize::MAX,
            chart_valid: false,
            chart_data: None,
            overlay: Overlay::None,
            profiles: std::collections::HashMap::new(),
            search_mode: false,
            search_input: String::new(),
            finding: None,
            hits: Vec::new(),
            hidx: usize::MAX,
            filter_mode: false,
            filter_input: String::new(),
            active_filter: None,
            export_mode: false,
            export_input: String::new(),
            x_col: None,
            heatmap: false,
            pinned_cols: 0,
            col_widths: std::collections::HashMap::new(),
            msg: None,
            quit: false,
        }
    }



    fn goto(&mut self, row: usize, col: usize) {
        self.sel.row = row;
        self.sel.col = col;
        self.hcol = col;
        self.chart_valid = false;
    }

    /// Jump to the next (or previous) hit, wrapping around the hit list.
    fn step_hit(&mut self, fwd: bool) {
        if self.hits.is_empty() {
            return;
        }
        self.hidx = match (self.hidx, fwd) {
            (usize::MAX, true) => 0,
            (usize::MAX, false) => self.hits.len() - 1,
            (i, true) => (i + 1) % self.hits.len(),
            (i, false) => {
                if i == 0 {
                    self.hits.len() - 1
                } else {
                    i - 1
                }
            }
        };
        let h = self.hits[self.hidx];
        self.goto(h.row, h.col);
        self.msg = Some((self.hit_label(), "ok"));
    }

    /// "i/n" position label for the current hit.
    pub fn hit_label(&self) -> String {
        if self.hidx == usize::MAX || self.hits.is_empty() {
            String::new()
        } else {
            format!("hit {}/{}", self.hidx + 1, self.hits.len())
        }
    }

    pub fn profile(&mut self, ci: usize) -> &crate::data::Profile {
        if !self.profiles.contains_key(&ci) {
            let p = crate::data::profile(&self.data, ci);
            self.profiles.insert(ci, p);
        }
        self.profiles.get(&ci).unwrap()
    }

    pub fn handle_key(&mut self, ev: KeyEvent) {
        // Esc / q always closes overlays first, then clears message, then quits.
        let rows = self.data.df.height().max(1);
        let cols = self.data.cols.len().max(1);

        if self.overlay != Overlay::None {
            if let Overlay::TablePicker { tables, selected } = &mut self.overlay {
                match ev.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('m') => {
                        self.overlay = Overlay::None;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *selected == 0 {
                            *selected = tables.len().saturating_sub(1);
                        } else {
                            *selected -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1) % tables.len().max(1);
                    }
                    KeyCode::Enter => {
                        let target_table = tables[*selected].clone();
                        self.overlay = Overlay::None;
                        if let Some(src) = &self.source_path {
                            let full_hint = format!("{}::{}", src.display(), target_table);
                            match crate::data::load(std::path::Path::new(&full_hint)) {
                                Ok(new_data) => {
                                    self.data = new_data;
                                    self.sel = Cell { row: 0, col: 0 };
                                    self.hcol = 0;
                                    self.chart_valid = false;
                                    self.profiles.clear();
                                    self.x_col = None;
                                    self.active_filter = None;
                                    self.msg = Some((format!("switched to table \"{}\"", target_table), "green"));
                                }
                                Err(e) => {
                                    self.msg = Some((format!("failed to load table: {e}"), "red"));
                                }
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }

            match ev.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('p') | KeyCode::Enter => {
                    self.overlay = Overlay::None
                }
                _ => {}
            }
            return;
        }

        if self.search_mode {
            match ev.code {
                KeyCode::Esc => {
                    self.search_mode = false;
                    self.search_input.clear();
                }
                KeyCode::Enter => {
                    self.search_mode = false;
                    if self.search_input.is_empty() {
                        self.hits.clear();
                        self.finding = None;
                        self.hidx = usize::MAX;
                    } else {
                        let q = self.search_input.clone();
                        let from = self.sel;
                        self.hits = crate::data::find_all(&self.data, &q)
                            .into_iter()
                            .map(|(row, col)| Cell { row, col })
                            .collect();
                        self.finding = Some(q);
                        self.hidx = match self
                            .hits
                            .iter()
                            .position(|h| (h.row, h.col) >= (from.row, from.col))
                        {
                            Some(i) => i,
                            None if !self.hits.is_empty() => 0,
                            None => usize::MAX,
                        };
                        match self.hidx {
                            usize::MAX => {
                                self.msg = Some((format!("\"{}\" not found", self.finding.as_deref().unwrap_or("")), "err"));
                            }
                            i => {
                                let h = self.hits[i];
                                self.goto(h.row, h.col);
                                self.msg = Some((self.hit_label(), "ok"));
                            }
                        }
                    }
                }
                KeyCode::Backspace => {
                    self.search_input.pop();
                }
                KeyCode::Char(c) => self.search_input.push(c),
                _ => {}
            }
            return;
        }

        if self.filter_mode {
            match ev.code {
                KeyCode::Esc => {
                    self.filter_mode = false;
                    self.filter_input.clear();
                }
                KeyCode::Enter => {
                    self.filter_mode = false;
                    if self.filter_input.is_empty() {
                        self.data.df = self.data.orig_df.clone();
                        self.active_filter = None;
                        self.sel = Cell { row: 0, col: 0 };
                        self.chart_valid = false;
                        self.profiles.clear();
                        self.msg = Some(("filter cleared".to_string(), "ok"));
                    } else {
                        let q = self.filter_input.clone();
                        match crate::data::filter_data(&self.data, &q) {
                            Ok(filtered) => {
                                let cnt = filtered.height();
                                self.data.df = filtered;
                                self.active_filter = Some(q);
                                self.sel = Cell { row: 0, col: 0 };
                                self.chart_valid = false;
                                self.profiles.clear();
                                self.msg = Some((format!("filtered: {} / {} rows", cnt, self.data.orig_df.height()), "green"));
                            }
                            Err(e) => {
                                self.msg = Some((format!("filter error: {e}"), "red"));
                            }
                        }
                    }
                }
                KeyCode::Backspace => {
                    self.filter_input.pop();
                }
                KeyCode::Char(c) => self.filter_input.push(c),
                _ => {}
            }
            return;
        }

        if self.export_mode {
            match ev.code {
                KeyCode::Esc => {
                    self.export_mode = false;
                    self.export_input.clear();
                }
                KeyCode::Enter => {
                    self.export_mode = false;
                    let target = self.export_input.clone();
                    match crate::data::export(&self.data.df, &target) {
                        Ok(()) => {
                            self.msg = Some((format!("saved {} rows to {}", self.data.df.height(), target), "green"));
                        }
                        Err(e) => {
                            self.msg = Some((format!("export failed: {e}"), "red"));
                        }
                    }
                }
                KeyCode::Backspace => {
                    self.export_input.pop();
                }
                KeyCode::Char(c) => self.export_input.push(c),
                _ => {}
            }
            return;
        }

        if let Some((_, _)) = &self.msg {
            self.msg = None;
            return; // dismiss flash with any key
        }




        if ev.modifiers.contains(KeyModifiers::CONTROL) {
            match ev.code {
                KeyCode::Char('c') | KeyCode::Char('d') => self.quit = true,
                KeyCode::Char('u') => {
                    self.sel.row = self.sel.row.saturating_sub(4);
                }
                KeyCode::Char('f') | KeyCode::Char('v') => {
                    self.sel.row = (self.sel.row + 4).min(rows - 1);
                }
                _ => {}
            }
            return;
        }

        match ev.code {
            KeyCode::Esc => {
                self.hits.clear();
                self.finding = None;
                self.hidx = usize::MAX;
                if self.active_filter.is_some() {
                    self.data.df = self.data.orig_df.clone();
                    self.active_filter = None;
                    self.sel = Cell { row: 0, col: 0 };
                    self.chart_valid = false;
                    self.profiles.clear();
                    self.msg = Some(("filter cleared".to_string(), "ok"));
                }
            }

            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_input.clear();
            }
            KeyCode::Char('n') => self.step_hit(true),
            KeyCode::Char('N') => self.step_hit(false),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') | KeyCode::F(1) => self.overlay = Overlay::Help,
            KeyCode::Char('s') => {
                let ci = self.sel.col;
                if let Err(e) = self.data.sort_by_col(ci, true) {
                    self.msg = Some((format!("sort failed: {e}"), "red"));
                } else {
                    self.chart_valid = false;
                    self.profiles.clear();
                    if let Some(q) = &self.finding {
                        self.hits = crate::data::find_all(&self.data, q)
                            .into_iter()
                            .map(|(row, col)| Cell { row, col })
                            .collect();
                    }
                    let cname = &self.data.cols[ci].name;
                    self.msg = Some((format!("sorted by \"{}\" asc [▲]", cname), "green"));
                }
            }
            KeyCode::Char('S') => {
                let ci = self.sel.col;
                if let Err(e) = self.data.sort_by_col(ci, false) {
                    self.msg = Some((format!("sort failed: {e}"), "red"));
                } else {
                    self.chart_valid = false;
                    self.profiles.clear();
                    if let Some(q) = &self.finding {
                        self.hits = crate::data::find_all(&self.data, q)
                            .into_iter()
                            .map(|(row, col)| Cell { row, col })
                            .collect();
                    }
                    let cname = &self.data.cols[ci].name;
                    self.msg = Some((format!("sorted by \"{}\" desc [▼]", cname), "green"));
                }
            }
            KeyCode::Char('u') => {
                let filter_cleared = self.active_filter.is_some();
                if filter_cleared {
                    self.data.df = self.data.orig_df.clone();
                    self.active_filter = None;
                    self.sel = Cell { row: 0, col: 0 };
                    self.chart_valid = false;
                    self.profiles.clear();
                }
                let sort_cleared = self.data.sort_state.is_some();
                if sort_cleared {
                    self.data.reset_sort();
                    self.chart_valid = false;
                    self.profiles.clear();
                }
                if filter_cleared || sort_cleared {
                    if let Some(q) = &self.finding {
                        self.hits = crate::data::find_all(&self.data, q)
                            .into_iter()
                            .map(|(row, col)| Cell { row, col })
                            .collect();
                    }
                    let m = if filter_cleared && sort_cleared {
                        "reset sort and filter"
                    } else if sort_cleared {
                        "reset sort order"
                    } else {
                        "reset filter"
                    };
                    self.msg = Some((m.to_string(), "ok"));
                }
            }
            KeyCode::Char('x') => {
                if self.x_col == Some(self.sel.col) {
                    self.x_col = None;
                    self.chart_valid = false;
                    self.msg = Some(("unset X-axis (reset to row index)".to_string(), "ok"));
                } else {
                    self.x_col = Some(self.sel.col);
                    self.chart_valid = false;
                    let cname = &self.data.cols[self.sel.col].name;
                    self.msg = Some((format!("set X-axis to \"{}\"", cname), "green"));
                }
            }
            KeyCode::Char('H') => {
                self.heatmap = !self.heatmap;
                self.msg = Some((
                    if self.heatmap {
                        "heatmap mode ON"
                    } else {
                        "heatmap mode OFF"
                    }
                    .to_string(),
                    "ok",
                ));
            }
            KeyCode::Char('f') => {
                self.filter_mode = true;
                self.filter_input.clear();
            }
            KeyCode::Char('e') => {
                self.export_mode = true;
                let stem = self.data.name.split('.').next().unwrap_or("export");
                self.export_input = format!("{}_export.csv", stem);
            }
            KeyCode::Char('m') => {
                if let Some(src) = &self.source_path {
                    match crate::sources::list_tables(src) {
                        Ok(tables) if tables.len() > 1 => {
                            self.overlay = Overlay::TablePicker { tables, selected: 0 };
                        }
                        Ok(_) => {
                            self.msg = Some(("single-table file (no other tables)".to_string(), "ok"));
                        }
                        Err(e) => {
                            self.msg = Some((format!("list tables failed: {e}"), "red"));
                        }
                    }
                } else {
                    self.msg = Some(("stdin input (no table picker available)".to_string(), "ok"));
                }
            }
            KeyCode::Char('F') => {
                if self.pinned_cols > 0 {
                    self.pinned_cols = 0;
                    self.msg = Some(("unpinned columns".to_string(), "ok"));
                } else {
                    let count = (self.sel.col + 1).min(self.data.cols.len());
                    self.pinned_cols = count;
                    self.msg = Some((
                        format!("pinned {} col{}", count, if count == 1 { "" } else { "s" }),
                        "green",
                    ));
                }
            }
            KeyCode::Char('<') => {
                let ci = self.sel.col;
                let cur = self.col_widths.get(&ci).copied().unwrap_or(12);
                let new_w = cur.saturating_sub(2).max(3);
                self.col_widths.insert(ci, new_w);
                self.msg = Some((format!("col width: {}", new_w), "ok"));
            }
            KeyCode::Char('>') => {
                let ci = self.sel.col;
                let cur = self.col_widths.get(&ci).copied().unwrap_or(12);
                let new_w = (cur + 2).min(80);
                self.col_widths.insert(ci, new_w);
                self.msg = Some((format!("col width: {}", new_w), "ok"));
            }
            KeyCode::Char('=') => {
                let ci = self.sel.col;
                if self.col_widths.remove(&ci).is_some() {
                    self.msg = Some(("reset col width to auto".to_string(), "ok"));
                }
            }
            KeyCode::Char('y') => {
                if !self.data.cols.is_empty() && self.data.df.height() > 0 {
                    let colname = &self.data.cols[self.sel.col].name;
                    let kind = self.data.cols[self.sel.col].kind;
                    if let Ok(col) = self.data.df.column(colname) {
                        let s = col.as_materialized_series();
                        let text = crate::data::cell_string(s, self.sel.row, kind);
                        crate::clipboard::copy_to_clipboard(&text);
                        let preview = crate::ui::trunc(&text, 20);
                        self.msg = Some((format!("yanked cell: \"{}\"", preview), "green"));
                    }
                }
            }
            KeyCode::Char('Y') => {
                if !self.data.cols.is_empty() && self.data.df.height() > 0 {
                    let mut row_vals = Vec::new();
                    for c in &self.data.cols {
                        if let Ok(col) = self.data.df.column(&c.name) {
                            let s = col.as_materialized_series();
                            let val = crate::data::cell_string(s, self.sel.row, c.kind);
                            if val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r') {
                                row_vals.push(format!("\"{}\"", val.replace('"', "\"\"")));
                            } else {
                                row_vals.push(val);
                            }
                        }
                    }
                    let line = row_vals.join(",");
                    crate::clipboard::copy_to_clipboard(&line);
                    self.msg = Some((format!("yanked row {} as CSV", self.sel.row + 1), "green"));
                }
            }
            KeyCode::Char('p') => self.overlay = Overlay::Profile(self.sel.col),
            KeyCode::Char('t') => self.chart_on = !self.chart_on,
            KeyCode::Char('c') => self.chart_kind = next_kind(self.chart_kind),


            KeyCode::PageDown => {
                self.sel.row = (self.sel.row + 10).min(rows - 1);
                self.chart_valid = false;
            }
            KeyCode::PageUp => {
                self.sel.row = self.sel.row.saturating_sub(10);
                self.chart_valid = false;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.sel.row = self.sel.row.saturating_sub(1);
                self.chart_valid = false;
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
                self.sel.row = (self.sel.row + 1).min(rows - 1);
                self.chart_valid = false;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.sel.col = self.sel.col.saturating_sub(1);
                self.hcol = self.sel.col;
                self.chart_valid = false;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.sel.col = (self.sel.col + 1).min(cols - 1);
                self.chart_valid = false;
            }
            KeyCode::Tab => {
                self.sel.col = (self.sel.col + 1).min(cols - 1);
                self.chart_valid = false;
            }
            KeyCode::BackTab => {
                self.sel.col = self.sel.col.saturating_sub(1);
                self.hcol = self.sel.col;
                self.chart_valid = false;
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.sel.row = 0;
                self.chart_valid = false;
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.sel.row = rows - 1;
                self.chart_valid = false;
            }
            KeyCode::Char('0') => {
                self.sel.col = 0;
                self.hcol = 0;
                self.chart_valid = false;
            }
            _ => {}
        }
    }
}

fn next_kind(k: ChartKind) -> ChartKind {
    match k {
        ChartKind::Hist => ChartKind::Line,
        ChartKind::Line => ChartKind::Scatter,
        ChartKind::Scatter => ChartKind::Hist,
        other => other,
    }
}
