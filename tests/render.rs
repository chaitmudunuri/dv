use dv::{app::App, data, ui};

use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn render(path: &str, keys: &[char]) -> String {
    let d = data::load(std::path::Path::new(path)).expect("load");
    let mut a = App::new(d);
    for k in keys {
        // reuse handle_key by synthesizing events
        let ev = crossterm::event::KeyEvent::new(
            match k {
                'h' => crossterm::event::KeyCode::Char('h'),
                'l' => crossterm::event::KeyCode::Char('l'),
                'k' => crossterm::event::KeyCode::Char('k'),
                'j' => crossterm::event::KeyCode::Char('j'),
                'p' => crossterm::event::KeyCode::Char('p'),
                'c' => crossterm::event::KeyCode::Char('c'),
                'r' => crossterm::event::KeyCode::Enter,
                'N' => crossterm::event::KeyCode::Char('N'),
                'g' => crossterm::event::KeyCode::Char('g'),
                'G' => crossterm::event::KeyCode::Char('G'),
                other => crossterm::event::KeyCode::Char(*other),
            },
            crossterm::event::KeyModifiers::empty(),
        );
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
    let s = render("samples/demo.csv", &['/', 'N', 'Y', 'C', 'r', 'l']);
    assert!(s.contains("/NYC"), "committed query chip in status");
    assert!(s.contains("[n] next hit"), "keys line advertises n");

    // startup row should have jumped to a NYC row
    assert!(s.contains("city=NYC"), "cursor jumped to a hit cell");
}

#[test]
fn search_no_hits_zero_chip_and_no_panic() {
    // query with no matches: "not found" flash on commit, then a 0-hits chip
    let s = render("samples/demo.csv", &['/', 'z', 'z', 'z', 'r', 'l']);
    assert!(s.contains("/zzz 0 hits"), "zero-hit chip shows after dismissal");
    assert!(s.contains("[n] next hit"), "no keys-line regression");
}
