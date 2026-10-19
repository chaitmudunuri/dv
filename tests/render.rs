use dv::{app::App, data, ui};

use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn render(path: &str, keys: &[char]) -> String {
    let d = data::load(std::path::Path::new(path)).expect("load");
    let mut a = App::with_path(d, Some(std::path::PathBuf::from(path)));
    for k in keys {
        // reuse handle_key by synthesizing events
        let code = match k {
            '\n' => crossterm::event::KeyCode::Enter,
            '\x1b' => crossterm::event::KeyCode::Esc,
            '\t' => crossterm::event::KeyCode::Tab,
            '\x08' => crossterm::event::KeyCode::Backspace,
            other => crossterm::event::KeyCode::Char(*other),
        };
        let ev = crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::empty());
        a.handle_key(ev);
    }
    let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
    term.draw(|f| ui::render_frame(f, &mut a)).unwrap();
    let mut out = String::new();
    for y in 0..30 {
        for x in 0..110 {
            let c = term
                .backend()
                .buffer()
                .cell(ratatui::layout::Position::from((x as u16, y as u16)))
                .unwrap()
                .symbol();
            out.push_str(c);
        }
        out.push('\n');
    }
    out
}

#[test]
fn grid_renders_and_chart_toggles() {
    // chart pane is off by default
    let s = render("samples/demo.csv", &[]);
    assert!(s.contains("demo.csv"), "file name in status");
    assert!(s.contains("A") && s.contains("B"), "col letters");
    assert!(!s.contains("HIST"), "no chart pane by default");

    // toggle on → histogram
    let s = render("samples/demo.csv", &['t']);
    assert!(s.contains("HIST"), "chart title");

    // line chart
    let s = render("samples/demo.csv", &['t', 'c']);
    assert!(s.contains("LINE"), "line chart title feasible");

    // text column → top-k
    let s = render("samples/demo.csv", &['t', 'l', 'l']);
    assert!(s.contains("TOP"), "top-k chart title");

    // profile overlay
    let s = render("samples/demo.csv", &['p']);
    assert!(s.contains("PROFILE"), "profile overlay");

    // help overlay
    let s = render("samples/demo.csv", &['?']);
    assert!(s.contains("move cursor"), "help overlay");

    // parquet
    let s = render("samples/demo.parquet", &['t']);
    assert!(s.contains("HIST"), "parquet hist");

    let s = render("samples/demo.arrow", &['t']);
    assert!(s.contains("HIST"), "arrow hist");
}

#[test]
fn search_prompt_lives_typing_and_hint() {
    // `/` opens the prompt and echoes typed characters
    let s = render("samples/demo.csv", &['/', 'N', 'Y']);
    assert!(s.contains(" search "), "search prompt shown");
    assert!(s.contains("/NY"), "typed query echoed");
    assert!(s.contains("[enter] find"), "prompt hint");
    assert!(!s.contains("· /NY 0 hits"), "no committed chip while typing");
}

#[test]
fn search_commit_jumps_and_chips() {
    // "NYC" appears deeper in the file; committing jumps there and flashes
    // the hit label (any key dismisses the flash, then the query chip shows)
    let s = render("samples/demo.csv", &['/', 'N', 'Y', 'C', '\n', 'l']);
    assert!(s.contains("/NYC"), "committed query chip in status");
    assert!(s.contains("[n] next hit"), "keys line advertises n");

    // startup row should have jumped to a NYC row
    assert!(s.contains("city=NYC"), "cursor jumped to a hit cell");
}

#[test]
fn search_no_hits_zero_chip_and_no_panic() {
    // query with no matches: "not found" flash on commit, then a 0-hits chip
    let s = render("samples/demo.csv", &['/', 'z', 'z', 'z', '\n', 'l']);
    assert!(s.contains("/zzz 0 hits"), "zero-hit chip shows after dismissal");
    assert!(s.contains("[n] next hit"), "no keys-line regression");
}

#[test]
fn sorting_asc_desc_and_undo() {
    let s = render("samples/demo.csv", &['s']);
    assert!(s.contains("A▲"), "A▲ in header when sorted asc");
    assert!(s.contains("sorted by"), "flash msg confirms asc sort");

    let s = render("samples/demo.csv", &['S']);
    assert!(s.contains("A▼"), "A▼ in header when sorted desc");
    assert!(s.contains("sorted by"), "flash msg confirms desc sort");

    let s = render("samples/demo.csv", &['s', '\n', 'u']);
    assert!(!s.contains("A▲") && !s.contains("A▼"), "header reset after undo sort");
    assert!(s.contains("reset sort order"), "flash msg confirms undo");
}

#[test]
fn column_pinning_and_freeze() {
    let s = render("samples/demo.csv", &['F']);
    assert!(s.contains("pinned 1 col"), "flash msg on pin");
    assert!(s.contains('│'), "vertical separator divider rendered for pinned col");

    let s = render("samples/demo.csv", &['F', '\n', 'F']);
    assert!(s.contains("unpinned"), "flash msg on unpin");
    assert!(!s.contains('│'), "separator removed after unpin");
}

#[test]
fn column_width_adjustment() {
    let s = render("samples/demo.csv", &['>']);
    assert!(s.contains("col width:"), "widened col width message");

    let s = render("samples/demo.csv", &['>', '\n', '=']);
    assert!(s.contains("reset col width to auto"), "reset col width message");
}

#[test]
fn clipboard_yank_cell_and_row() {
    let s = render("samples/demo.csv", &['y']);
    assert!(s.contains("yanked cell"), "cell yank message");

    let s = render("samples/demo.csv", &['Y']);
    assert!(s.contains("yanked row"), "row yank message");
}

#[test]
fn profile_extended_quantiles_and_box_plot() {
    let s = render("samples/demo.csv", &['p']);
    assert!(s.contains("PROFILE"), "profile overlay opened");
    assert!(s.contains("median"), "median in profile stats");
    assert!(s.contains("p25") && s.contains("p75"), "quartiles in profile");
    assert!(s.contains("p90") && s.contains("p99"), "higher percentiles in profile");
    assert!(s.contains("iqr"), "interquartile range in profile");
    assert!(s.contains("box & whisker"), "box & whisker plot title");
    assert!(s.contains('├') && s.contains('┤'), "box plot whiskers");
}

#[test]
fn bivariate_charting_x_column() {
    let s = render("samples/demo.csv", &['t', 'x']);
    assert!(s.contains("set X-axis to \"age\""), "x-col set message");

    // Turn on line chart with 'c', set X on col A ('x'), dismiss msg ('\n'), move to col B ('l')
    let s = render("samples/demo.csv", &['t', 'c', 'x', '\n', 'l']);
    assert!(s.contains("A[X]"), "header contains [X] badge for column A");
    assert!(s.contains("vs A age"), "chart title indicates bivariate head vs X");

    // unset X-axis
    let s = render("samples/demo.csv", &['t', 'x', '\n', 'h', 'x']);
    assert!(s.contains("unset X-axis"), "x-axis toggle off message");
}

#[test]
fn heatmap_conditional_formatting() {
    let s = render("samples/demo.csv", &['H']);
    assert!(s.contains("heatmap mode ON"), "heatmap on message");

    let s = render("samples/demo.csv", &['H', '\n']);
    assert!(s.contains("[HEATMAP]"), "heatmap badge in status bar");

    let s = render("samples/demo.csv", &['H', '\n', 'H']);
    assert!(s.contains("heatmap mode OFF"), "heatmap off message");
}

#[test]
fn filtering_substring_and_numeric_condition() {
    // filter mode prompt
    let s = render("samples/demo.csv", &['f']);
    assert!(s.contains(" filter "), "filter prompt shown");

    // filter by substring "NYC"
    let s = render("samples/demo.csv", &['f', 'N', 'Y', 'C', '\n', 'l']);
    assert!(s.contains("[filter: NYC]"), "active filter badge");

    let s = render("samples/demo.csv", &['f', 's', 'c', 'o', 'r', 'e', '>', '5', '0', '\n', 'l']);
    assert!(s.contains("[filter: score>50]"), "expression filter badge");

    // clear filter with 'u'
    let s = render("samples/demo.csv", &['f', 'N', 'Y', 'C', '\n', 'l', 'u']);
    assert!(s.contains("reset filter"), "filter reset message");
    assert!(!s.contains("[filter: NYC]"), "badge cleared after reset");
}

#[test]
fn export_current_view() {
    let target = "demo_export.csv";
    if std::path::Path::new(target).exists() {
        let _ = std::fs::remove_file(target);
    }
    let s = render("samples/demo.csv", &['e', '\n']);
    assert!(s.contains("saved"), "export success flash message");
    assert!(std::path::Path::new(target).exists(), "exported file was created");
    let content = std::fs::read_to_string(target).expect("read exported csv");
    assert!(content.contains("age,score,city,active,signup"), "correct csv header exported");
    let _ = std::fs::remove_file(target);
}

#[test]
fn table_picker_overlay() {
    let s = render("samples/demo.db", &['m']);
    assert!(s.contains("Select Table / Sheet"), "table picker overlay title");
    assert!(s.contains("events") && s.contains("small"), "lists both tables in demo.db");

    // press Enter to load table
    let s = render("samples/demo.db", &['m', '\n']);
    assert!(s.contains("switched to table"), "switched table flash message");
}

#[test]
fn command_palette_filter_and_execute() {
    let s = render("samples/demo.csv", &[':']);
    assert!(s.contains("Command Palette"), "palette overlay title");
    assert!(s.contains("conditional formatting"), "palette command listed");

    // Filter for heatmap and hit enter to execute
    let s = render("samples/demo.csv", &[':', 'h', 'e', 'a', 't', '\n']);
    assert!(s.contains("heatmap mode ON"), "heatmap flash message active after executing command");

    // Dismiss message and verify badge
    let s = render("samples/demo.csv", &[':', 'h', 'e', 'a', 't', '\n', 'j']);
    assert!(s.contains("[HEATMAP]"), "heatmap badge active after dismissing flash message");
}

#[test]
fn chart_layout_toggle_w() {
    let d = data::load(std::path::Path::new("samples/demo.csv")).expect("load");
    let mut a = App::with_path(d, Some(std::path::PathBuf::from("samples/demo.csv")));
    assert_eq!(a.chart_layout, dv::app::ChartLayout::Bottom);
    let ev = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('w'), crossterm::event::KeyModifiers::empty());
    a.handle_key(ev);
    assert_eq!(a.chart_layout, dv::app::ChartLayout::Right);
    a.msg = None;
    a.handle_key(ev);
    assert_eq!(a.chart_layout, dv::app::ChartLayout::Bottom);
}

#[test]
fn mouse_wheel_scrolling() {
    let d = data::load(std::path::Path::new("samples/demo.csv")).expect("load");
    let mut a = App::with_path(d, Some(std::path::PathBuf::from("samples/demo.csv")));
    assert_eq!(a.sel.row, 0);

    let scroll_down = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::ScrollDown,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    a.handle_mouse(scroll_down);
    assert_eq!(a.sel.row, 3, "scrolled down 3 rows");

    let scroll_up = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::ScrollUp,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };
    a.handle_mouse(scroll_up);
    assert_eq!(a.sel.row, 0, "scrolled back up");
}


