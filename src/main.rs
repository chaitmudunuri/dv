use dv::{app, data, ui};
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "dv",
    version,
    about = "VisiCalc-style quick visualization of data files in the terminal",
    long_about = "Opens an interactive terminal spreadsheet with charts for csv, tsv, parquet,\narrow/ipc, jsonl, avro, xlsx, sqlite, duckdb and gzipped csv/jsonl data.\n\nPass '-' (or pipe input) to read from stdin.\nUse --stats for a non-interactive summary dump."
)]
struct Cli {
    /// data file (csv/tsv/parquet/arrow/ipc/jsonl/avro/xlsx/sqlite/duckdb/gz),
    /// '-' for stdin, or postgres://user:pass@host/database
    file: Option<std::path::PathBuf>,

    /// print a per-column summary and exit (no TUI)
    #[arg(long)]
    stats: bool,
}

fn fmt(v: f64) -> String {
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = cli.file.unwrap_or_else(|| std::path::PathBuf::from("-"));
    let d = data::load(&path)?;

    if cli.stats {
        println!(
            "{}: {} rows × {} cols",
            d.name,
            d.df.height(),
            d.cols.len()
        );
        for (i, c) in d.cols.iter().enumerate() {
            let p = data::profile(&d, i);
            let tag = match c.kind {
                data::ColKind::Number => "NUM ",
                data::ColKind::Bool => "BOOL",
                data::ColKind::Text => "TXT ",
            };
            let detail = if let Some(st) = &p.num {
                format!(
                    "n={} mean={} σ={} min={} max={}",
                    st.count,
                    fmt(st.mean),
                    fmt(st.std),
                    fmt(st.min),
                    fmt(st.max)
                )
            } else if let Some(t) = &p.top {
                let dcnt = p.distinct.unwrap_or(0);
                let top = t
                    .first()
                    .map(|(n, c)| format!("\"{}\"×{}", n, c))
                    .unwrap_or_else(|| "—".into());
                format!("distinct={} top={}", dcnt, top)
            } else {
                "all null".into()
            };
            println!("  [{}] {:15} {}", tag, c.name, detail);
        }
        return Ok(());
    }

    let mut a = app::App::new(d);
    ui::run(&mut a)
}
