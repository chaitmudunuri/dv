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
}

pub struct App {
    pub data: Data,
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
    pub msg: Option<(String, &'static str)>,
    pub quit: bool,
}

impl App {
    pub fn new(data: Data) -> Self {
        App {
            data,
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
                        // first hit at/after the cursor, else first overall
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
                // clear active search highlights
                self.hits.clear();
                self.finding = None;
                self.hidx = usize::MAX;
            }
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_input.clear();
            }
            KeyCode::Char('n') => self.step_hit(true),
            KeyCode::Char('N') => self.step_hit(false),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') | KeyCode::F(1) => self.overlay = Overlay::Help,
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
            KeyCode::Char('G') | KeyCode::End | KeyCode::Char('e') => {
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
