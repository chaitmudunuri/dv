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

## Keys

| key | action |
|---|---|
| `h j k l` / arrows | move cursor |
| `Tab` / `BackTab` | next / previous cell |
| `g` / `G` | jump to top / bottom row |
| `PgUp` / `PgDn` | page 10 rows |
| `p` | profile the column under the cursor |
| `c` | cycle chart type (HIST → LINE → DOT) |
| `t` | toggle chart pane |
| `0` | jump to first column |
| `?` | help |
| `q` | quit |

Numeric columns show a histogram, line, or dot plot; text and bool
columns show a TOP-K bar chart.

## Install

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
