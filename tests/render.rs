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
                't' => crossterm::event::KeyCode::Char('t'),
                'g' => crossterm::event::KeyCode::Char('g'),
                'G' => crossterm::event::KeyCode::Char('G'),
                '?' => crossterm::event::KeyCode::Char('?'),
                'e' => crossterm::event::KeyCode::Char('e'),
                _ => crossterm::event::KeyCode::Char(*k),
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
fn grid_and_histogram_render() {
    let s = render("samples/demo.csv", &[]);
    assert!(s.contains("demo.csv"), "file name in status");
    assert!(s.contains("A") && s.contains("B"), "col letters");
    assert!(s.contains("HIST"), "chart title");

    // line chart
    let s = render("samples/demo.csv", &['c']);
    assert!(s.contains("LINE"), "line chart title feasible");

    // text column → top-k
    let s = render("samples/demo.csv", &['l', 'l']);
    assert!(s.contains("TOP"), "top-k chart title");

    // profile overlay
    let s = render("samples/demo.csv", &['p']);
    assert!(s.contains("PROFILE"), "profile overlay");

    // help overlay
    let s = render("samples/demo.csv", &['?']);
    assert!(s.contains("move cursor"), "help overlay");

    // parquet
    let s = render("samples/demo.parquet", &[]);
    assert!(s.contains("HIST"), "parquet hist");

    let s = render("samples/demo.arrow", &[]);
    assert!(s.contains("HIST"), "arrow hist");
}
