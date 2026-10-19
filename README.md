# dv

Terminal data visualizer — an interactive TUI spreadsheet for quick
charting and browsing of data files.

```sh
dv data.csv
```

`dv` opens an interactive terminal spreadsheet: a grid you navigate with
vim keys, plus a live chart pane and a profiler for whichever column the
cursor is on. No servers, no notebooks — just point it at a file.

## Formats

| category | formats |
|---|---|
| text | `csv`, `tsv` |
| columnar | `parquet`, `arrow` / `ipc` |
| row-json | `jsonl` / `ndjson` |
| other | `avro`, `xlsx` / `xls` / `ods` |
| databases | `sqlite` (`.db`), `duckdb` (`.duckdb`) |
| compressed | `csv.gz`, `tsv.gz`, `jsonl.gz` |
| over the network | `postgres://user:pass@host/database` |
| stdin | pipe anything text-like: `cat x.csv \| dv -` |

Csv/tsv/arrow/ipc can also be read from a pipe.
Multi-table files (sqlite, duckdb) auto-open the biggest table; open a
specific one with a `::table` suffix, e.g. `dv demo.duckdb::tips`.

## Keys & Controls

| key | action |
|---|---|
| `:` / `Ctrl+P` | **Command Palette** (fuzzy search & run any command) |
| `h j k l` / arrows | move cursor |
| `Mouse Wheel` | scroll data rows up / down smoothly |
| `Tab` / `BackTab` | next / previous cell |
| `/` | live incremental search (`n`/`N` next/prev hit, live hit counter pill) |
| `f` | filter rows (substring or `col>val`, `col=val`, `col!=val`) |
| `s` / `S` | sort column ascending / descending |
| `u` | reset sort order and active filter |
| `F` | freeze / unfreeze columns up to cursor |
| `<` / `>` | shrink / widen column width |
| `=` | reset column width to auto-fit |
| `H` | toggle conditional formatting / heatmap |
| `x` | toggle column as X-axis for bivariate chart ($X$ vs $Y$) |
| `w` | toggle chart layout (classic bottom split vs right sidebar split) |
| `e` | export current view to CSV, Parquet, or JSONL |
| `m` | table / sheet picker (SQLite, DuckDB, Excel) |
| `y` / `Y` | yank cell / row as CSV to clipboard |
| `g` / `G` | jump to top / bottom row |
| `PgUp` / `PgDn` | page 10 rows |
| `p` | profile column (stats, quantiles, sparkline, box plot) |
| `c` | cycle chart type (HIST → LINE → DOT) |
| `t` | toggle chart pane |
| `0` | jump to first column |
| `?` | help overlay |
| `q` | quit |

Numeric columns show a histogram, line, or dot plot; bivariate charting (`x`) plots two numeric columns against each other. Text and bool columns show a TOP-K bar chart. Column profiling (`p`) provides percentiles (`p25`, median, `p75`, `p90`, `p99`, `iqr`) and a Unicode box & whisker plot. Press `w` to toggle between bottom-split and right-sidebar chart layouts. Full mouse and trackpad scrolling is supported.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/chaitmudunuri/dv/master/install.sh | sh
```

Prebuilt binaries (linux x86_64/arm64, macOS x86_64/arm64, windows x86_64)
are attached to each [release](https://github.com/chaitmudunuri/dv/releases);
grab `dv-windows-x86_64.zip` from there on Windows. To install to another
location: `DV_INSTALL_DIR=/usr/local/bin curl -fsSL … | sh`.

Or from source:

```sh
git clone https://github.com/chaitmudunuri/dv.git
cd dv
cargo install --path .
```

Requires Rust 1.85+ (edition 2024).

## Non-interactive use

Pass `--stats` for a per-column summary and exit:

```sh
$ dv --stats data.csv
data.csv: 1200 rows × 5 cols
  [NUM ] age    n=1200 mean=116.8 σ=21.1 min=58.3 max=218.6
  [TXT ] city   distinct=4 top="NYC"×397
  [BOOL] active distinct=2 top="true"×845
  ...
```

## Building blocks

Built on [polars](https://github.com/pola-rs/polars) for data loading and
profiling, [ratatui](https://github.com/ratatui/ratatui) for the
terminal UI, and native clients for sqlite/duckdb/postgres (plus
[calamine](https://github.com/tafia/calamine) for spreadsheets).
