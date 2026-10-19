use crate::app::{App, Overlay};
use crate::data::{ChartData, ChartKind, ColKind, Profile};

use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Clear, Dataset, GraphType, Paragraph};
use ratatui::{symbols, Terminal};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const BLOCKS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

const DIM: Style = Style::new().fg(Color::DarkGray);
const NUM_FG: Style = Style::new().fg(Color::Cyan);
const CURSOR: Style = Style::new()
    .bg(Color::Rgb(70, 120, 220))
    .fg(Color::Black)
    .add_modifier(Modifier::BOLD);
const HIT: Style = Style::new()
    .bg(Color::Rgb(140, 90, 0))
    .fg(Color::White)
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
        crossterm::event::DisableMouseCapture
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
                _ => {}
            }
        }
    }
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        term.backend_mut(),
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
        let gh = (main.height * 5 / 8).max(4).min(main.height - 3);
        (
            Rect { height: gh, ..main },
            Rect { y: main.y + gh, height: main.height - gh, ..main },
        )
    } else {
        (main, Rect::default())
    };

    draw_grid(f, grid, app);
    if app.chart_on && overlays_none && chart_area.height > 3 {
        draw_chart_pane(f, chart_area, app);
    }
    draw_status(f, status, app);
    match app.overlay {
        Overlay::Profile(ci) => draw_profile(f, area, app, ci),
        Overlay::Help => draw_help(f, area),
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
        let style = if c.ci == app.sel.col {
            Style::new().bg(Color::Rgb(120, 120, 40)).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            DIM
        };
        spans.push(Span::styled(pad_align(letter, c.w, true), style));
        if pinned > 0 && c.ci + 1 == pinned && idx + 1 < cols.len() {
            spans.push(Span::styled("│", Style::new().fg(Color::Cyan)));
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
            CURSOR
        } else {
            DIM
        };
        spans.push(Span::styled(pad_align(num, gutter, true), row_style));
        for (idx, c) in cols.iter().enumerate() {
            let text = c.cells.get(rel).cloned().unwrap_or_default();
            let right = app.data.cols[c.ci].kind == ColKind::Number;
            let cell = pad_align(text.clone(), c.w, right);
            let is_hit = app
                .finding
                .as_ref()
                .map(|q| {
                    let lq = q.to_lowercase();
                    text.to_lowercase().contains(&lq)
                })
                .unwrap_or(false);
            let style = if c.ci == app.sel.col && row == app.sel.row {
                CURSOR
            } else if is_hit {
                HIT
            } else {
                Style::new()
            };
            spans.push(Span::styled(cell, style));
            if pinned > 0 && c.ci + 1 == pinned && idx + 1 < cols.len() {
                spans.push(Span::styled("│", Style::new().fg(Color::Cyan)));
            } else {
                spans.push(Span::raw(" "));
            }
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect { y: area.y + rel as u16 + 1, height: 1, ..area },
        );
    }
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
    app.chart_data = crate::data::chart(&app.data, ci, eff);
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
            title.push(Span::raw(format!(
                " {}  points={}  nulls={}/{}",
                head,
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
        .title(Line::from(title))
        .border_style(DIM);
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
    let n = pts.last().unwrap().0;
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for &(_, y) in pts {
        ymin = ymin.min(y);
        ymax = ymax.max(y);
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
                .bounds([0.0, n.max(1.0)])
                .labels(vec!["0".to_string(), n.to_string()])
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
                Style::new().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" /{}", app.search_input), Style::new().add_modifier(Modifier::BOLD)),
            Span::styled("  [enter] find · [esc] cancel ", DIM),
        ])
    } else if let Some((m, c)) = app.msg.clone() {
        Line::from(vec![Span::styled(
            format!(" {} ", m),
            Style::new().fg(Color::Black).bg(color_for(c)),
        )])
    } else {
        let mut spans = vec![
            Span::styled(
                format!(" {} ", app.data.name),
                Style::new().add_modifier(Modifier::BOLD),
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
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!("{}=", colname),
                Style::new().fg(Color::Yellow),
            ));
            spans.push(Span::styled(v, Style::new().add_modifier(Modifier::BOLD)));
        }
        if let Some(q) = &app.finding {
            spans.push(Span::styled(
                if app.hits.is_empty() {
                    format!("· /{q} 0 hits")
                } else {
                    format!("· /{q} {} hit{} ", app.hits.len(), if app.hits.len() == 1 { "" } else { "s" })
                },
                Style::new().fg(Color::Yellow),
            ));
        }
        Line::from(spans)
    };
    let keys = if app.search_mode {
        ""
    } else {
        " [j k l h] move · [/] search · [n] next hit · [s/S/u] sort · [F] freeze · [< > =] width · [y/Y] yank · [p] profile · [c] chart · [?] help · [q] quit "
    };


    f.render_widget(Paragraph::new(vec![line1]), Rect { height: 1, ..area });
    f.render_widget(
        Paragraph::new(Span::styled(keys, DIM)),
        Rect { y: area.y + 1, height: 1, ..area },
    );
}

fn color_for(c: &str) -> Color {
    match c {
        "red" => Color::Red,
        "green" => Color::Green,
        _ => Color::Yellow,
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
    let rects = popup(area, 70, 80);
    f.render_widget(Clear, rects);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::styled(
                format!(" PROFILE {} {} ", col_letter(ci), col.name),
                Style::new().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        ]))
        .border_style(DIM);
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
        Span::styled(format!(" {} ", kindtag), Style::new().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  rows {} · nulls {} ({})", total, p.nulls, pct(p.nulls as u64, total)), DIM),
    ]));
    lines.push(Line::from(""));
    if let Some(st) = &p.num {
        lines.push(Line::from(vec![
            Span::styled("min      ", DIM),
            Span::raw(fmt_num(st.min)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("max      ", DIM),
            Span::raw(fmt_num(st.max)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("mean     ", DIM),
            Span::raw(fmt_num(st.mean)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("σ (std)  ", DIM),
            Span::raw(fmt_num(st.std)),
        ]));
        lines.push(Line::from(""));
        if let Some(bins) = &p.hist {
            let w = 60usize.min(inner.width as usize);
            lines.push(Line::from(Span::styled(spark(bins, w), NUM_FG)));
            lines.push(Line::from(vec![
                Span::styled(fmt_num(st.min), DIM),
                Span::raw(" ".repeat(w.saturating_sub(fmt_num(st.min).width() + fmt_num(st.max).width()))),
                Span::raw(fmt_num(st.max)),
            ]));
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
    let l = lines.len();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(" esc/p/q: close ", DIM)));
    f.render_widget(
        Paragraph::new(lines),
        Rect { height: inner.height.min(l as u16 + 2), ..inner },
    );
}

const HELP: &[(&str, &str)] = &[
    ("h / j / k / l (←↑→↓)", "move cursor"),
    ("Tab / BackTab", "next / previous cell"),
    ("/", "search prompts for text"),
    ("n / N", "next / previous search hit"),
    ("esc", "clear search hits"),
    ("s / S / u", "sort asc / desc / reset sort"),
    ("F", "freeze / unfreeze columns up to cursor"),
    ("< / > / =", "shrink / widen / reset column width"),
    ("y / Y", "yank cell / row as CSV to clipboard"),
    ("g / G", "jump to top / bottom row"),
    ("PgUp / PgDn", "page 10 rows"),
    ("p", "profile column under cursor"),
    ("c", "cycle chart type (HIST→LINE→DOT)"),
    ("t", "toggle chart pane"),
    ("0", "jump to first column"),
    ("q", "quit"),
    ("?", "this help"),
];


fn draw_help(f: &mut ratatui::Frame, area: Rect) {
    let rects = popup(area, 60, 60);
    f.render_widget(Clear, rects);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " dv — keys ",
            Style::new().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
        ))
        .border_style(DIM);
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
