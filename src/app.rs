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
    pub msg: Option<(String, &'static str)>,
    pub quit: bool,
}

impl App {
    pub fn new(data: Data) -> Self {
        App {
            data,
            sel: Cell { row: 0, col: 0 },
            hcol: 0,
            chart_on: true,
            chart_kind: ChartKind::Hist,
            chart_col: usize::MAX,
            chart_valid: false,
            chart_data: None,
            overlay: Overlay::None,
            profiles: std::collections::HashMap::new(),
            msg: None,
            quit: false,
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
