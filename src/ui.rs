use crate::app::{App, ChartLayout, Overlay};
use crate::data::{ChartData, ChartKind, ColKind, Profile};

use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Chart, Clear, Dataset, GraphType, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState,
};
use ratatui::{symbols, Terminal};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const BLOCKS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

// Catppuccin-inspired modern palette
const BG_HEADER: Color = Color::Rgb(49, 50, 68);
const TEXT_PRIMARY: Color = Color::Rgb(205, 214, 244);
const TEXT_MUTED: Color = Color::Rgb(147, 153, 178);
const BORDER_COLOR: Color = Color::Rgb(88, 91, 112);
const ACCENT_CYAN: Color = Color::Rgb(137, 220, 235);
const ACCENT_PEACH: Color = Color::Rgb(250, 179, 135);
const ACCENT_MAUVE: Color = Color::Rgb(203, 166, 247);
const ACCENT_GREEN: Color = Color::Rgb(166, 227, 161);
const ACCENT_YELLOW: Color = Color::Rgb(249, 226, 175);
const ACCENT_RED: Color = Color::Rgb(243, 139, 168);

const DIM: Style = Style::new().fg(TEXT_MUTED);
const NUM_FG: Style = Style::new().fg(ACCENT_CYAN);
const CURSOR: Style = Style::new()
    .bg(Color::Rgb(69, 71, 90))
    .fg(Color::Rgb(245, 224, 220))
    .add_modifier(Modifier::BOLD);
const HIT: Style = Style::new()
    .bg(ACCENT_YELLOW)
    .fg(Color::Black)
    .add_modifier(Modifier::BOLD);

// ---------- helpers ----------

pub fn col_letter(idx: usize) -> String {
    let mut n = idx as u32 + 1;
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    out.reverse();
    out.into_iter().collect()
}

fn digits(n: usize) -> usize {
    let mut d = 1;
    let mut x = n / 10;
    while x > 0 {
        d += 1;
        x /= 10;
    }
    d
}

fn fmt_num(v: f64) -> String {
    let a = v.abs();
    if a != 0.0 && (a >= 1e12 || a < 1e-3) {
        return format!("{:e}", v);
    }
    if a >= 1e6 {
        return format!("{:.0}", v);
    }
    let s = format!("{:.4}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn trunc(s: &str, w: usize) -> String {

    if w == 0 {
        return String::new();
    }
    if s.width() <= w {
        return s.to_string();
    }
    let mut acc = String::new();
    let mut cw = 0;
    for c in s.chars() {
        let cwi = UnicodeWidthChar::width(c).unwrap_or(1);
        if cw + cwi > w.saturating_sub(1) {
            break;
        }
        acc.push(c);
        cw += cwi;
    }
    acc.push('…');
    acc
}

fn pad_align(s: String, w: usize, right: bool) -> String {
    let sw = s.width();
    if sw >= w {
        return trunc(&s, w);
    }
    let fill = " ".repeat(w - sw);
    if right {
        format!("{}{}", fill, s)
    } else {
        format!("{}{}", s, fill)
    }
}

fn spark(vals: &[u64], width: usize) -> String {
    if vals.is_empty() || width == 0 {
        return String::new();
    }
    let max = vals.iter().copied().max().unwrap_or(1).max(1) as f64;
    (0..width)
        .map(|i| {
            let oi = (i as u64 * vals.len() as u64) / width as u64;
            let v = vals[oi.min(vals.len() as u64 - 1) as usize];
            BLOCKS[((v as f64 / max) * 7.99) as usize].to_string()
        })
        .collect()
}

fn pct(part: u64, total: u64) -> String {
    if total == 0 {
        return "0.0%".into();
    }
    format!("{:.1}%", part as f64 / total as f64 * 100.0)
}

// ---------- event loop ----------

pub fn run(app: &mut App) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        &mut stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout))?;
    let tick = std::time::Duration::from_millis(80);
    while !app.quit {
        term.draw(|f| render_frame(f, app))?;
        if crossterm::event::poll(tick)? {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(k)
                    if k.kind == crossterm::event::KeyEventKind::Press =>
                {
                    app.handle_key(k)
                }
                crossterm::event::Event::Mouse(m) => {
                    app.handle_mouse(m);
                }
                _ => {}
            }
        }
    }
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        term.backend_mut(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    Ok(())
}

// ---------- main layout ----------

pub fn render_frame(f: &mut ratatui::Frame, app: &mut App) {
    let area = f.area();
    if area.width < 8 || area.height < 5 {
        return;
    }
    let main = Rect { height: area.height - 2, ..area };
    let status = Rect { y: area.y + main.height, height: 2, ..area };

    let overlays_none = app.overlay == Overlay::None;
    let (grid, chart_area) = if app.chart_on && overlays_none {
        match app.chart_layout {
            ChartLayout::Bottom => {
                let gh = (main.height * 5 / 8).max(4).min(main.height - 3);
                (
                    Rect { height: gh, ..main },
                    Rect { y: main.y + gh, height: main.height - gh, ..main },
                )
            }
            ChartLayout::Right => {
                let gw = (main.width * 5 / 8).max(20).min(main.width.saturating_sub(20));
                (
                    Rect { width: gw, ..main },
                    Rect { x: main.x + gw, width: main.width - gw, ..main },
                )
            }
        }
    } else {
        (main, Rect::default())
    };

    draw_grid(f, grid, app);
    if app.chart_on && overlays_none && (chart_area.height > 3 || chart_area.width > 10) {
        draw_chart_pane(f, chart_area, app);
    }
    draw_status(f, status, app);
    match &app.overlay {
        Overlay::Profile(ci) => draw_profile(f, area, app, *ci),
        Overlay::Help => draw_help(f, area),
        Overlay::TablePicker { tables, selected } => draw_table_picker(f, area, tables, *selected),
        Overlay::CommandPalette { query, selected } => draw_command_palette(f, area, query, *selected),
        Overlay::None => {}
    }
}


// ---------- grid ----------

struct ColBlock {
    ci: usize,
    w: usize,
    cells: Vec<String>,
}

fn build_one_col(
    app: &App,
    ci: usize,
    scroll_row: usize,
    rows_v: usize,
    avail: usize,
) -> Option<ColBlock> {
    if avail < 3 {
        return None;
    }
    let name = app.data.cols[ci].name.clone();
    let s = app
        .data
        .df
        .column(name.as_str())
        .ok()?
        .as_materialized_series()
        .clone();
    let kind = app.data.cols[ci].kind;
    let mut cells = Vec::with_capacity(rows_v);
    for r in scroll_row..(scroll_row + rows_v).min(app.data.df.height()) {
        let mut t = crate::data::cell_string(&s, r, kind);
        t.replace_in_place();
        cells.push(t);
    }
    let w = if let Some(&ow) = app.col_widths.get(&ci) {
        ow.min(avail)
    } else {
        let mut w = cells.iter().map(|c| c.width()).max().unwrap_or(0);
        w = w.max(col_letter(ci).width() + 2).max(4).min(24);
        w.min(avail)
    };
    if w == 0 {
        return None;
    }
    Some(ColBlock { ci, w, cells })
}

fn build_cols(
    app: &App,
    scroll_row: usize,
    rows_v: usize,
    start: usize,
    width: u16,
    gutter: usize,
) -> Vec<ColBlock> {
    let mut out = Vec::new();
    let mut used = gutter as u16;
    let pinned = app.pinned_cols.min(app.data.cols.len());

    // 1. Pinned columns first (0..pinned)
    for ci in 0..pinned {
        let avail = width.saturating_sub(used);
        if avail < 4 {
            break;
        }
        if let Some(cb) = build_one_col(app, ci, scroll_row, rows_v, (avail as usize).saturating_sub(1)) {
            used += (cb.w + 1) as u16;
            out.push(cb);
        } else {
            break;
        }
    }

    if pinned > 0 && !out.is_empty() {
        used += 1;
    }

    // 2. Unpinned columns (start.max(pinned)..app.data.cols.len())
    let mut ci = start.max(pinned);
    while ci < app.data.cols.len() {
        let avail = width.saturating_sub(used);
        if avail < 4 {
            break;
        }
        if let Some(cb) = build_one_col(app, ci, scroll_row, rows_v, (avail as usize).saturating_sub(1)) {
            used += (cb.w + 1) as u16;
            out.push(cb);
            ci += 1;
        } else {
            break;
        }
    }
    out
}

trait InPlace {
    fn replace_in_place(&mut self);
}
impl InPlace for String {
    fn replace_in_place(&mut self) {
        if self.contains(['\n', '\r', '\t']) {
            *self = self.replace(['\n', '\r', '\t'], " ");
        }
    }
}

fn draw_grid(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let cols_total = app.data.cols.len();
    if cols_total == 0 {
        f.render_widget(Paragraph::new("no columns"), area);
        return;
    }
    let rows_total = app.data.df.height();
    if rows_total == 0 {
        f.render_widget(Paragraph::new("empty dataset (0 rows)"), area);
        return;
    }

    let rows_v = area.height as usize; // includes header line
    let data_rows_v = rows_v.saturating_sub(1);
    let gutter = digits(rows_total).max(2) + 1;
    let scroll_row = if app.sel.row >= data_rows_v {
        app.sel.row + 1 - data_rows_v
    } else {
        0
    };

    let pinned = app.pinned_cols.min(app.data.cols.len());
    let mut cols = build_cols(app, scroll_row, data_rows_v, app.hcol, area.width, gutter);
    if !cols.iter().any(|c| c.ci == app.sel.col) {
        app.hcol = app.sel.col;
        cols = build_cols(app, scroll_row, data_rows_v, app.hcol, area.width, gutter);
    }
    if cols.is_empty() {
        f.render_widget(Paragraph::new("terminal too narrow"), area);
        return;
    }

    // header letter row (VisiCalc-style A, B, C …)
    let mut spans = vec![Span::styled(" ".repeat(gutter), DIM)];
    for (idx, c) in cols.iter().enumerate() {
        let mut letter = col_letter(c.ci);
        if let Some((sci, asc)) = app.data.sort_state {
            if sci == c.ci {
                letter.push(if asc { '▲' } else { '▼' });
            }
        }
        if app.x_col == Some(c.ci) {
            letter = format!("{}[X]", letter);
        }
        let style = if c.ci == app.sel.col {
            Style::new().bg(ACCENT_PEACH).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)
        };
        spans.push(Span::styled(pad_align(letter, c.w, true), style));
        if pinned > 0 && c.ci + 1 == pinned && idx + 1 < cols.len() {
            spans.push(Span::styled("│", Style::new().fg(ACCENT_CYAN).add_modifier(Modifier::BOLD)));
        } else {
            spans.push(Span::raw(" "));
        }
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect { y: area.y, height: 1, ..area },
    );

    // data rows
    for rel in 0..data_rows_v {
        let row = scroll_row + rel;
        if row >= rows_total {
            break;
        }
        let mut spans = Vec::with_capacity(cols.len() * 2 + 1);
        let num = (row + 1).to_string();
        let row_style = if row == app.sel.row {
            Style::new().fg(ACCENT_PEACH).add_modifier(Modifier::BOLD)
        } else {
            DIM
        };
        spans.push(Span::styled(pad_align(num, gutter, true), row_style));
        for (idx, c) in cols.iter().enumerate() {
            let text = c.cells.get(rel).cloned().unwrap_or_default();
            let right = app.data.cols[c.ci].kind == ColKind::Number;
            let cell = pad_align(text.clone(), c.w, right);
            let is_hit = (app.search_mode && !app.search_input.is_empty() && text.to_lowercase().contains(&app.search_input.to_lowercase()))
                || app.finding.as_ref().map(|q| text.to_lowercase().contains(&q.to_lowercase())).unwrap_or(false);
            let is_null = text.is_empty() || text == "null";
            let style = if c.ci == app.sel.col && row == app.sel.row {
                CURSOR
            } else if is_hit {
                HIT
            } else if is_null {
                DIM
            } else if app.heatmap && app.data.cols[c.ci].kind == ColKind::Number {
                if let Ok(v) = text.trim().parse::<f64>() {
                    let st = app.profile(c.ci).num;
                    if let Some(st) = st {
                        if st.max > st.min {
                            let ratio = ((v - st.min) / (st.max - st.min)).clamp(0.0, 1.0);
                            let (fg, bg) = if ratio < 0.25 {
                                (ACCENT_CYAN, Color::Rgb(25, 40, 55))
                            } else if ratio < 0.50 {
                                (ACCENT_GREEN, Color::Rgb(30, 50, 40))
                            } else if ratio < 0.75 {
                                (ACCENT_YELLOW, Color::Rgb(55, 50, 25))
                            } else {
                                (ACCENT_RED, Color::Rgb(65, 30, 40))
                            };
                            Style::new().fg(fg).bg(bg)
                        } else {
                            Style::new()
                        }
                    } else {
                        Style::new()
                    }
                } else {
                    Style::new()
                }
            } else {
                Style::new()
            };
            spans.push(Span::styled(cell, style));
            if pinned > 0 && c.ci + 1 == pinned && idx + 1 < cols.len() {
                spans.push(Span::styled("│", Style::new().fg(ACCENT_CYAN).add_modifier(Modifier::BOLD)));
            } else {
                spans.push(Span::raw(" "));
            }
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect { y: area.y + rel as u16 + 1, height: 1, ..area },
        );
    }

    // Modern vertical scrollbar
    let mut scrollbar_state = ScrollbarState::new(rows_total.saturating_sub(1)).position(app.sel.row);
    f.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
            .thumb_symbol("█")
            .style(Style::new().fg(BORDER_COLOR)),
        Rect { y: area.y + 1, height: area.height.saturating_sub(1), ..area },
        &mut scrollbar_state,
    );
}


// ---------- chart pane ----------

fn ensure_chart(app: &mut App) {
    let ci = app.sel.col;
    let eff = if app.data.cols.get(ci).map(|c| c.kind) == Some(ColKind::Number) {
        app.chart_kind
    } else {
        ChartKind::TopK
    };
    let kind_compatible = match &app.chart_data {
        Some(ChartData::Hist { .. }) | Some(ChartData::Series { .. }) => eff == app.chart_kind,
        Some(ChartData::TopK { .. }) => eff == ChartKind::TopK,
        None => false,
    };
    if app.chart_valid && app.chart_col == ci && kind_compatible {
        return;
    }
    app.chart_data = crate::data::chart(&app.data, ci, eff, app.x_col);
    app.chart_col = ci;
    app.chart_valid = true;
}

fn draw_chart_pane(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    ensure_chart(app);
    let ci = app.sel.col;
    let colname = app.data.cols[ci].name.clone();
    let kind = app.data.cols[ci].kind;
    let p = app.profile(ci).clone();
    let head = format!("{} {}", col_letter(ci), colname);
    let mut title = vec![Span::styled(
        format!(" {} ", title_kind(kind, app.chart_kind)),
        Style::new().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
    )];
    match &app.chart_data {
        Some(ChartData::Hist { stats, .. }) => {
            title.push(Span::raw(format!(
                " {}  n={}  mean={} σ={}  nulls={}/{}",
                head,
                stats.count,
                fmt_num(stats.mean),
                fmt_num(stats.std),
                p.nulls,
                app.data.df.height()
            )));
        }
        Some(ChartData::Series { pts }) => {
            let label = if let Some(xi) = app.x_col {
                if xi < app.data.cols.len() && xi != ci {
                    format!("{} vs {} {}", head, col_letter(xi), app.data.cols[xi].name)
                } else {
                    head
                }
            } else {
                head
            };
            title.push(Span::raw(format!(
                " {}  points={}  nulls={}/{}",
                label,
                pts.len(),
                p.nulls,
                app.data.df.height()
            )));
        }

        Some(ChartData::TopK { total, items }) => {
            let from = items.iter().take(5).map(|(_, c)| *c).sum::<u64>();
            title.push(Span::raw(format!(
                " {}  distinct={}  top5={}% of non-null {}",
                head,
                p.distinct.unwrap_or(0),
                pct(from, *total),
                *total
            )));
        }
        None => {}
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(title))
        .border_style(Style::new().fg(BORDER_COLOR));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 1 {
        return;
    }
    match &app.chart_data {
        Some(ChartData::Hist { bins, stats }) => draw_hist(f, inner, bins, stats),
        Some(ChartData::Series { pts }) => draw_series(
            f,
            inner,
            pts,
            app.chart_kind == ChartKind::Line,
        ),
        Some(ChartData::TopK { items, total }) => draw_topk(f, inner, items, *total),
        None => f.render_widget(Paragraph::new("no chart for this column"), inner),
    }
}

fn title_kind(kind: ColKind, k: ChartKind) -> &'static str {
    match kind {
        ColKind::Number => match k {
            ChartKind::Hist => "HIST",
            ChartKind::Line => "LINE",
            ChartKind::Scatter => "DOT",
            _ => "",
        },
        _ => "TOP",
    }
}

fn draw_hist(f: &mut ratatui::Frame, area: Rect, bins: &[u64], stats: &crate::data::NumStats) {
    let bars_h = area.height.saturating_sub(1) as usize;
    if bars_h == 0 || area.width == 0 {
        return;
    }
    let nbars = bins.len();
    let width = area.width as usize;
    let max = bins.iter().copied().max().unwrap_or(1).max(1) as f64;

    // aggregate bins to fit width; rows[0] is the top line
    let mut rows = vec![String::new(); bars_h];
    for i in 0..width {
        let oi = (i as u64 * nbars as u64) / width as u64;
        let cnt = bins[oi.min(nbars as u64 - 1) as usize] as f64 / max * bars_h as f64;
        for y in 0..bars_h {
            let level = (bars_h - y) as f64; // 1.0 = bottom cell
            let fill = cnt - (level - 1.0);
            let ch = if fill >= 1.0 {
                '█'
            } else if fill > 0.0 {
                BLOCKS[(fill * 7.999) as usize]
            } else {
                ' '
            };
            rows[y].push(ch);
        }
    }
    let mut lines = Vec::with_capacity(bars_h + 1);
    for r in &rows {
        lines.push(Line::from(vec![Span::styled(r.clone(), NUM_FG)]));
    }
    // axis labels row
    let lab: Vec<Span> = {
        let left = fmt_num(stats.min);
        let right = fmt_num(stats.max);
        let lw = left.width();
        let rw = right.width();
        if lw + rw >= width {
            vec![Span::styled(left, DIM)]
        } else {
            vec![
                Span::styled(left, DIM),
                Span::styled(" ".repeat(width - lw - rw), DIM),
                Span::styled(right, DIM),
            ]
        }
    };
    lines.push(Line::from(lab));
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_series(f: &mut ratatui::Frame, area: Rect, pts: &[(f64, f64)], line: bool) {
    if pts.is_empty() {
        f.render_widget(Paragraph::new("no data").style(DIM), area);
        return;
    }
    let mut xmin = f64::INFINITY;
    let mut xmax = f64::NEG_INFINITY;
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for &(x, y) in pts {
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    if (xmax - xmin).abs() < f64::EPSILON {
        let pad = (xmax.abs() * 0.1).max(1.0);
        xmin -= pad;
        xmax += pad;
    } else {
        let pad = (xmax - xmin) * 0.04;
        xmin -= pad;
        xmax += pad;
    }
    if (ymax - ymin).abs() < f64::EPSILON {
        let pad = (ymax.abs() * 0.1).max(1.0);
        ymin -= pad;
        ymax += pad;
    } else {
        let pad = (ymax - ymin) * 0.06;
        ymin -= pad;
        ymax += pad;
    }
    let ds = Dataset::default()
        .marker(if line { symbols::Marker::Dot } else { symbols::Marker::HalfBlock })
        .graph_type(if line { GraphType::Line } else { GraphType::Scatter })
        .style(Style::new().fg(Color::Cyan))
        .data(pts);
    let chart = Chart::new(vec![ds])
        .style(Style::new())
        .x_axis(
            Axis::default()
                .bounds([xmin, xmax])
                .labels(vec![fmt_num(xmin), fmt_num(xmax)])
                .style(DIM),
        )
        .y_axis(
            Axis::default()
                .bounds([ymin, ymax])
                .labels(vec![fmt_num(ymin), fmt_num(ymax)])
                .style(DIM),
        );
    f.render_widget(chart, area);
}

fn draw_topk(f: &mut ratatui::Frame, area: Rect, items: &[(String, u64)], total: u64) {
    let mut lines = Vec::new();
    let max = items.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
    let labelw = 18usize.min(area.width as usize / 2);
    let barw = (area.width as usize).saturating_sub(labelw + 8);
    for (name, count) in items.iter().take(area.height as usize) {
        let name = pad_align(trunc(name, labelw), labelw, false);
        let bn = (*count as f64 / max as f64 * barw as f64).round() as usize;
        let p = pct(*count, total);
        lines.push(Line::from(vec![
            Span::raw(name),
            Span::styled("█".repeat(bn.min(barw)), NUM_FG),
            Span::styled(format!(" {:>6}", p), DIM),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("(no values)", DIM)));
    }
    f.render_widget(Paragraph::new(lines), area);
}

// ---------- status ----------

fn draw_status(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let ci = app.sel.col;
    let row = app.sel.row;
    let line1 = if app.search_mode {
        Line::from(vec![
            Span::styled(
                " search ",
                Style::new().fg(Color::Black).bg(ACCENT_PEACH).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" /{}", app.search_input), Style::new().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(
                if app.search_input.is_empty() {
                    "".to_string()
                } else {
                    format!("  [{} hit{}]", app.hits.len(), if app.hits.len() == 1 { "" } else { "s" })
                },
                Style::new().fg(ACCENT_YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  [enter] find · [esc] cancel ", DIM),
        ])
    } else if app.filter_mode {
        Line::from(vec![
            Span::styled(
                " filter ",
                Style::new().fg(Color::Black).bg(ACCENT_CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}", app.filter_input), Style::new().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("  text or col>val · [enter] apply (blank resets) · [esc] cancel ", DIM),
        ])
    } else if app.export_mode {
        Line::from(vec![
            Span::styled(
                " export ",
                Style::new().fg(Color::Black).bg(ACCENT_GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}", app.export_input), Style::new().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("  filename (.csv, .parquet, .jsonl) · [enter] export · [esc] cancel ", DIM),
        ])
    } else if let Some((m, c)) = app.msg.clone() {
        Line::from(vec![Span::styled(
            format!(" {} ", m),
            Style::new().fg(Color::Black).bg(color_for(c)).add_modifier(Modifier::BOLD),
        )])
    } else {
        let mut spans = vec![
            Span::styled(
                format!(" {} ", app.data.name),
                Style::new().bg(BG_HEADER).fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("· {}x{} ", app.data.df.height(), app.data.cols.len()),
                DIM,
            ),
        ];
        if !app.data.cols.is_empty() && app.data.df.height() > 0 {
            let colname = app.data.cols[ci].name.clone();
            let kind = app.data.cols[ci].kind;
            let s = app
                .data
                .df
                .column(colname.as_str())
                .expect("columns are stable")
                .as_materialized_series();
            let v = crate::data::cell_string(s, row, kind);
            spans.push(Span::styled(
                format!("· {}{} ", col_letter(ci), row + 1),
                Style::new().fg(ACCENT_PEACH).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!("{}=", colname),
                Style::new().fg(ACCENT_PEACH),
            ));
            spans.push(Span::styled(v, Style::new().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)));
        }
        if let Some(q) = &app.finding {
            spans.push(Span::styled(
                if app.hits.is_empty() {
                    format!(" · /{q} 0 hits")
                } else {
                    format!(" · /{q} {} hit{} ", app.hits.len(), if app.hits.len() == 1 { "" } else { "s" })
                },
                Style::new().fg(ACCENT_YELLOW).add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(f) = &app.active_filter {
            spans.push(Span::styled(
                format!(" · [filter: {f}]"),
                Style::new().fg(ACCENT_CYAN).add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(xi) = app.x_col {
            if xi < app.data.cols.len() {
                spans.push(Span::styled(
                    format!(" · [X: {}]", app.data.cols[xi].name),
                    Style::new().fg(ACCENT_GREEN).add_modifier(Modifier::BOLD),
                ));
            }
        }
        if app.heatmap {
            spans.push(Span::styled(
                " · [HEATMAP]",
                Style::new().fg(ACCENT_MAUVE).add_modifier(Modifier::BOLD),
            ));
        }
        let pct_pos = if app.data.df.height() > 0 {
            format!(" · {}%", (row + 1) * 100 / app.data.df.height())
        } else {
            String::new()
        };
        if !pct_pos.is_empty() {
            spans.push(Span::styled(pct_pos, DIM));
        }
        Line::from(spans)
    };
    let keys = if app.search_mode || app.filter_mode || app.export_mode {
        ""
    } else {
        " [:] palette · [j k l h] move · [/] search · [n] next hit · [f] filter · [s/S/u] sort · [F] freeze · [H] heatmap · [e] export · [m] tables · [p] profile · [c] chart · [?] help · [q] quit "
    };


    f.render_widget(Paragraph::new(vec![line1]), Rect { height: 1, ..area });
    f.render_widget(
        Paragraph::new(Span::styled(keys, DIM)),
        Rect { y: area.y + 1, height: 1, ..area },
    );
}

fn color_for(c: &str) -> Color {
    match c {
        "red" => ACCENT_RED,
        "green" => ACCENT_GREEN,
        _ => ACCENT_YELLOW,
    }
}

// ---------- overlays ----------

fn popup(area: Rect, wpct: u16, hpct: u16) -> Rect {
    let w = area.width * wpct / 100;
    let h = area.height * hpct / 100;
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn draw_profile(f: &mut ratatui::Frame, area: Rect, app: &mut App, ci: usize) {
    let p: Profile = app.profile(ci).clone();
    let col = app.data.cols[ci].clone();
    let rects = popup(area, 70, 85);
    f.render_widget(Clear, rects);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(vec![
            Span::styled(
                format!(" PROFILE {} {} ", col_letter(ci), col.name),
                Style::new().fg(Color::Black).bg(ACCENT_PEACH).add_modifier(Modifier::BOLD),
            ),
        ]))
        .border_style(Style::new().fg(BORDER_COLOR));
    let inner = block.inner(rects);
    f.render_widget(block, rects);
    let mut lines = Vec::new();
    let total = app.data.df.height() as u64;
    let kindtag = match col.kind {
        ColKind::Number => "NUM",
        ColKind::Bool => "BOOL",
        ColKind::Text => "TXT",
    };
    lines.push(Line::from(vec![
        Span::styled(format!(" {} ", kindtag), Style::new().bg(ACCENT_CYAN).fg(Color::Black).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  rows {} · nulls {} ({})", total, p.nulls, pct(p.nulls as u64, total)), DIM),
    ]));
    lines.push(Line::from(""));
    if let Some(st) = &p.num {
        lines.push(Line::from(vec![
            Span::styled("min      ", DIM),
            Span::raw(format!("{:<14}", fmt_num(st.min))),
            Span::styled("p25      ", DIM),
            Span::raw(fmt_num(st.p25)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("max      ", DIM),
            Span::raw(format!("{:<14}", fmt_num(st.max))),
            Span::styled("median   ", DIM),
            Span::raw(fmt_num(st.p50)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("mean     ", DIM),
            Span::raw(format!("{:<14}", fmt_num(st.mean))),
            Span::styled("p75      ", DIM),
            Span::raw(fmt_num(st.p75)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("σ (std)  ", DIM),
            Span::raw(format!("{:<14}", fmt_num(st.std))),
            Span::styled("p90      ", DIM),
            Span::raw(fmt_num(st.p90)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("iqr      ", DIM),
            Span::raw(format!("{:<14}", fmt_num(st.iqr))),
            Span::styled("p99      ", DIM),
            Span::raw(fmt_num(st.p99)),
        ]));
        lines.push(Line::from(""));
        let w = 55usize.min(inner.width as usize);
        if let Some(bins) = &p.hist {
            lines.push(Line::from(Span::styled("histogram", DIM)));
            lines.push(Line::from(Span::styled(spark(bins, w), NUM_FG)));
            lines.push(Line::from(vec![
                Span::styled(fmt_num(st.min), DIM),
                Span::raw(" ".repeat(w.saturating_sub(fmt_num(st.min).width() + fmt_num(st.max).width()))),
                Span::raw(fmt_num(st.max)),
            ]));
        }
        if w >= 10 {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("box & whisker", DIM)));
            lines.push(Line::from(Span::styled(box_plot(st, w), ACCENT_YELLOW)));
        }
    }
    if let Some(items) = &p.top {
        if !items.is_empty() {
            lines.push(Line::from(Span::styled("top values", DIM)));
            let max = items.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
            for (n, c) in items.iter().take(10) {
                let bn = (*c as f64 / max as f64 * 12.0).round() as usize;
                lines.push(Line::from(vec![
                    Span::raw(pad_align(trunc(n, 24), 24, false)),
                    Span::styled("█".repeat(bn), NUM_FG),
                    Span::styled(format!(" {:>6}", pct(*c, total)), DIM),
                ]));
            }
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(" esc/p/q: close ", DIM)));
    f.render_widget(Paragraph::new(lines), inner);
}

fn box_plot(st: &crate::data::NumStats, width: usize) -> String {
    if width < 5 || st.count == 0 {
        return String::new();
    }
    let span = st.max - st.min;
    if span <= f64::EPSILON {
        return format!("├{}┤", "─".repeat(width.saturating_sub(2)));
    }
    let map = |v: f64| -> usize {
        let frac = ((v - st.min) / span).clamp(0.0, 1.0);
        (frac * (width - 1) as f64).round() as usize
    };

    let p_min = 0;
    let p_q1 = map(st.p25).min(width - 1);
    let p_med = map(st.p50).min(width - 1);
    let p_q3 = map(st.p75).min(width - 1);
    let p_max = width - 1;

    let mut chars = vec![' '; width];
    // whiskers
    for i in p_min..=p_q1 {
        chars[i] = '─';
    }
    chars[0] = '├';

    for i in p_q3..=p_max {
        chars[i] = '─';
    }
    chars[width - 1] = '┤';

    // box interior
    for i in p_q1..=p_q3 {
        chars[i] = '░';
    }
    if p_q1 == p_q3 {
        chars[p_q1] = '│';
    } else {
        chars[p_med] = '│';
        chars[p_q1] = '[';
        chars[p_q3] = ']';
    }

    chars.into_iter().collect()
}

fn draw_table_picker(f: &mut ratatui::Frame, area: Rect, tables: &[String], selected: usize) {
    let h = (tables.len() as u16 + 5).clamp(8, 20);
    let rects = popup(area, 50, (h * 100 / area.height.max(1)).clamp(30, 80));
    f.render_widget(Clear, rects);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " Select Table / Sheet ",
            Style::new().fg(Color::Black).bg(ACCENT_PEACH).add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::new().fg(BORDER_COLOR));
    let inner = block.inner(rects);
    f.render_widget(block, rects);

    let mut lines = Vec::new();
    for (i, t) in tables.iter().enumerate() {
        if i == selected {
            lines.push(Line::from(vec![
                Span::styled(" ► ", Style::new().fg(ACCENT_PEACH).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" {} ", t), Style::new().fg(Color::Black).bg(ACCENT_PEACH).add_modifier(Modifier::BOLD)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::raw(t.clone()),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(" [j/k/↑/↓] navigate · [enter] load · [esc/q] cancel ", DIM)));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_command_palette(f: &mut ratatui::Frame, area: Rect, query: &str, selected: usize) {
    let rects = popup(area, 60, 65);
    f.render_widget(Clear, rects);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " Command Palette ",
            Style::new().fg(Color::Black).bg(ACCENT_PEACH).add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::new().fg(BORDER_COLOR));
    let inner = block.inner(rects);
    f.render_widget(block, rects);

    if inner.height < 4 {
        return;
    }

    let q = query.to_lowercase();
    let filtered: Vec<&crate::app::PaletteItem> = crate::app::PALETTE_COMMANDS
        .iter()
        .filter(|c| {
            c.title.to_lowercase().contains(&q)
                || c.id.to_lowercase().contains(&q)
                || c.key.eq_ignore_ascii_case(&q)
                || c.category.to_lowercase().contains(&q)
        })
        .collect();

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(" > ", Style::new().fg(ACCENT_PEACH).add_modifier(Modifier::BOLD)),
        Span::styled(
            if query.is_empty() { "type to filter commands..." } else { query },
            if query.is_empty() { DIM } else { Style::new().fg(Color::White).add_modifier(Modifier::BOLD) },
        ),
    ]));
    lines.push(Line::from(Span::styled("─".repeat(inner.width as usize), Style::new().fg(BORDER_COLOR))));

    let max_items = (inner.height as usize).saturating_sub(4);
    let scroll_offset = if selected >= max_items { selected + 1 - max_items } else { 0 };

    for (i, item) in filtered.iter().enumerate().skip(scroll_offset).take(max_items) {
        let is_sel = i == selected;
        let prefix = if is_sel { " ► " } else { "   " };
        let category_tag = format!("[{}]", item.category);
        let title_w = (inner.width as usize).saturating_sub(prefix.len() + category_tag.len() + item.key.len() + 6);
        let truncated_title = trunc(item.title, title_w);
        let padding = " ".repeat(title_w.saturating_sub(truncated_title.width()));

        let style = if is_sel {
            Style::new().fg(Color::Black).bg(ACCENT_PEACH).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(TEXT_PRIMARY)
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, Style::new().fg(ACCENT_PEACH).add_modifier(Modifier::BOLD)),
            Span::styled(truncated_title, style),
            Span::raw(padding),
            Span::styled(format!(" {:<9} ", category_tag), DIM),
            Span::styled(format!(" [{}] ", item.key), Style::new().fg(ACCENT_CYAN).add_modifier(Modifier::BOLD)),
        ]));
    }

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled("   No matching commands", DIM)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(" [↑/↓/Tab] navigate · [enter] run · [esc] close ", DIM)));
    f.render_widget(Paragraph::new(lines), inner);
}

const HELP: &[(&str, &str)] = &[
    ("h / j / k / l (←↑→↓)", "move cursor"),
    ("Tab / BackTab", "next / previous cell"),
    (":", "command palette (omnibar)"),
    ("Ctrl+p", "command palette (omnibar)"),
    ("/", "search prompts for text"),
    ("n / N", "next / previous search hit"),
    ("f", "filter rows (substring or col>val, col=val)"),
    ("s / S / u", "sort asc / desc / reset sort & filter"),
    ("F", "freeze / unfreeze columns up to cursor"),
    ("< / > / =", "shrink / widen / reset column width"),
    ("H", "toggle conditional formatting / heatmap"),
    ("y / Y", "yank cell / row as CSV to clipboard"),
    ("e", "export current view to CSV, Parquet, or JSONL"),
    ("m", "table / sheet picker (multi-table files)"),
    ("g / G", "jump to top / bottom row"),
    ("PgUp / PgDn", "page 10 rows"),
    ("p", "profile column (stats, quantiles, box plot)"),
    ("c", "cycle chart type (HIST→LINE→DOT)"),
    ("w", "toggle chart layout (bottom/right split)"),
    ("x", "toggle column as X-axis for bivariate chart"),
    ("t", "toggle chart pane"),
    ("0", "jump to first column"),
    ("q", "quit"),
    ("?", "this help"),
];


fn draw_help(f: &mut ratatui::Frame, area: Rect) {
    let rects = popup(area, 60, 65);
    f.render_widget(Clear, rects);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " dv — Modern Terminal Visualizer ",
            Style::new().fg(Color::Black).bg(ACCENT_PEACH).add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::new().fg(BORDER_COLOR));
    let mut lines = Vec::new();
    for (k, d) in HELP {
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<24}", k), NUM_FG),
            Span::styled(*d, DIM),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
        " formats: csv tsv parquet arrow/ipc jsonl avro xlsx sqlite duckdb gz · postgres://u:p@host/db",
    ),
    DIM,
)));
    f.render_widget(Paragraph::new(lines), block.inner(rects));
    f.render_widget(block, rects);
}
