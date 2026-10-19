use anyhow::{bail, Context, Result};
use polars::prelude::*;
use std::io::{IsTerminal, Read};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColKind {
    Number,
    Bool,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartKind {
    Hist,
    Line,
    Scatter,
    TopK,
}

#[derive(Debug, Clone, Copy)]
pub struct NumStats {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub std: f64,
    pub count: u64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p90: f64,
    pub p99: f64,
    pub iqr: f64,
}


#[derive(Debug, Clone)]
pub struct ColInfo {
    pub name: String,
    pub kind: ColKind,
}

pub struct Data {
    pub df: DataFrame,
    pub orig_df: DataFrame,
    pub cols: Vec<ColInfo>,
    pub name: String,
    pub sort_state: Option<(usize, bool)>, // (col_idx, ascending)
}


#[derive(Debug, Clone)]
pub struct Profile {
    pub nulls: usize,
    pub num: Option<NumStats>,
    pub top: Option<Vec<(String, u64)>>,
    pub distinct: Option<u64>,
    pub hist: Option<Vec<u64>>,
}

pub enum ChartData {
    Hist {
        bins: Vec<u64>,
        stats: NumStats,
    },
    Series {
        pts: Vec<(f64, f64)>,
    },
    TopK {
        items: Vec<(String, u64)>,
        total: u64,
    },
}

fn kind_of(dt: &DataType) -> ColKind {
    match dt {
        DataType::Boolean => ColKind::Bool,
        DataType::Float32
        | DataType::Float64
        | DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64 => ColKind::Number,
        _ => ColKind::Text,
    }
}

pub fn load(path: &Path) -> Result<Data> {
    if let Some(uri) = crate::sources::uri_is_postgres(path) {
        return crate::sources::load_postgres(&uri);
    }
    let (path, table_hint) = crate::sources::split_table_hint(path);
    let from_stdin = path == std::path::Path::new("-");
    if from_stdin {
        if std::io::stdin().is_terminal() {
            bail!(
                "no input: give a file (csv/parquet/arrow/avro/xlsx/sqlite/duckdb/jsonl) or pipe data, e.g. cat x.csv | dv -"
            );
        }
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        let cursor = std::io::Cursor::new(buf.into_bytes());
        let df = CsvReadOptions::default()
            .with_has_header(true)
            .with_infer_schema_length(Some(4_096))
            .into_reader_with_file_handle(cursor)
            .finish()?;
        return build(df, true, "stdin".to_string());
    }
    if let Some(d) = special_load(&path, table_hint.clone())? {
        return Ok(d);
    }
    build(dispatch_file(&path)?, false, name_for(&path))
}

fn name_for(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub(crate) fn sniff_db(path: &Path) -> &'static str {
    use std::io::Read;
    let mut magic = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open(path) {
        if f.read_exact(&mut magic).is_ok() {
            if magic.starts_with(b"SQLite format 3\0") {
                return "sqlite";
            }
            if magic.starts_with(b"DUCK") {
                return "duckdb";
            }
        }
    }
    "unknown"
}

/// Gzipped children: decompress to memory, then route on the inner extension.
fn load_gz(path: &Path) -> Result<Data> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let bytes = std::fs::read(path)?;
    let mut cursor = std::io::Cursor::new(bytes);
    {
        let mut dec = GzDecoder::new(&mut cursor);
        let mut raw = Vec::new();
        dec.read_to_end(&mut raw)?;
        cursor.set_position(0);
        raw.shrink_to_fit();
        let inner = path
            .file_stem()
            .and_then(|s| Path::new(s).extension())
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let name = name_for(path);
        match inner.as_str() {
            "json" | "ndjson" | "jsonl" => {
                let df = JsonReader::new(std::io::Cursor::new(raw))
                    .with_json_format(JsonFormat::JsonLines)
                    .finish()?;
                build(df, false, name)
            }
            "tsv" | "tab" => {
                let df = CsvReadOptions::default()
                    .with_has_header(true)
                    .with_infer_schema_length(Some(4_096))
                    .with_parse_options(CsvParseOptions::default().with_separator(b'\t'))
                    .into_reader_with_file_handle(std::io::Cursor::new(raw))
                    .finish()?;
                build(df, false, name)
            }
            _ => {
                let df = CsvReadOptions::default()
                    .with_has_header(true)
                    .with_infer_schema_length(Some(4_096))
                    .into_reader_with_file_handle(std::io::Cursor::new(raw))
                    .finish()?;
                build(df, false, name)
            }
        }
    }
}

/// Formats that produce a `Data` directly (own naming/typing rules).
fn special_load(path: &Path, table_hint: Option<String>) -> Result<Option<Data>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let d = match ext.as_str() {
        "gz" => {
            return Ok(Some(load_gz(path)?));
        }
        "avro" => {
            let file = std::fs::File::open(path)?;
            let df = polars::io::avro::AvroReader::new(file).finish()?;
            build(df, false, name_for(path))?
        }
        "xlsx" | "xls" | "xlsm" | "xlsb" | "ods" => crate::sources::load_excel(path, table_hint)?,

        "db" | "sqlite" | "sqlite3" | "ddb" | "duckdb" => match sniff_db(path) {
            "sqlite" => crate::sources::load_sqlite(path, table_hint)?,
            _ => crate::sources::load_duckdb(path, table_hint)?,
        },
        _ => {
            if sniff_db(path) == "sqlite" {
                crate::sources::load_sqlite(path, table_hint)?
            } else {
                return Ok(None);
            }
        }
    };
    Ok(Some(d))
}

fn dispatch_file(path: &Path) -> Result<DataFrame> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let lf = match ext.as_str() {
        "csv" => LazyCsvReader::new(path)
            .with_has_header(true)
            .with_infer_schema_length(Some(4096))
            .finish(),
        "tsv" | "tab" => LazyCsvReader::new(path)
            .with_has_header(true)
            .with_infer_schema_length(Some(4096))
            .with_separator(b'\t')
            .finish(),
        "parquet" | "pq" => LazyFrame::scan_parquet(path, ScanArgsParquet::default()),
        "arrow" | "ipc" | "feather" | "arrows" => {
            LazyFrame::scan_ipc(path, ScanArgsIpc::default())
        }
        "json" | "ndjson" | "jsonl" => LazyJsonLineReader::new(path).finish(),
        _ => LazyCsvReader::new(path)
            .with_has_header(true)
            .with_infer_schema_length(Some(4096))
            .finish(),
    };
    lf.with_context(|| format!("scan {}", path.display()))?
        .collect()
        .with_context(|| format!("parse {}", path.display()))
}

pub fn build(mut df: DataFrame, _from_stdin: bool, name: String) -> Result<Data> {
    let names: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    for name in &names {
        let s = df.column(name)?;
        let cast_fmt = match s.dtype() {
            DataType::Date => Some("%Y-%m-%d"),
            DataType::Datetime(_, _) => Some("%Y-%m-%d %H:%M:%S"),
            DataType::Time => Some("%H:%M:%S"),
            _ => None,
        };
        if let Some(fmt) = cast_fmt {
            let out = df
                .clone()
                .lazy()
                .select([col(name.clone()).dt().to_string(fmt)])
                .collect()?;
            let s = out
                .column(name)?
                .as_materialized_series()
                .clone();
            df.with_column(s.into_column())
                .with_context(|| format!("cast {}", name))?;
        }
    }
    let mut cols = Vec::with_capacity(names.len());
    for n in &names {
        let dt = df.column(n)?.dtype().clone();
        cols.push(ColInfo {
            name: n.clone(),
            kind: kind_of(&dt),
        });
    }
    let orig_df = df.clone();
    Ok(Data {
        df,
        orig_df,
        cols,
        name,
        sort_state: None,
    })
}

impl Data {
    pub fn sort_by_col(&mut self, col_idx: usize, ascending: bool) -> Result<()> {
        if col_idx >= self.cols.len() {
            return Ok(());
        }
        let col_name = self.cols[col_idx].name.clone();
        let sorted = self.df.sort(
            [col_name.as_str()],
            SortMultipleOptions::default().with_order_descending(!ascending),
        )?;
        self.df = sorted;
        self.sort_state = Some((col_idx, ascending));
        Ok(())
    }

    pub fn reset_sort(&mut self) {
        self.df = self.orig_df.clone();
        self.sort_state = None;
    }
}


// ---- value extraction ----

pub fn numbers(s: &Series) -> Vec<f64> {
    match s.cast(&DataType::Float64) {
        Ok(c) => {
            let mut out = Vec::with_capacity(s.len());
            if let Ok(ca) = c.f64() {
                ca.iter().filter_map(|o| o).for_each(|v| out.push(v));
            }
            out
        }
        Err(_) => Vec::new(),
    }
}

pub fn strings(s: &Series) -> Vec<String> {
    let mut out = Vec::with_capacity(s.len());
    if matches!(s.dtype(), DataType::Boolean) {
        if let Ok(ca) = s.bool() {
            ca.iter().filter_map(|o| o).for_each(|v| out.push(v.to_string()));
            return out;
        }
    }
    if let Ok(ca) = s.str() {
        ca.iter().filter_map(|o| o).for_each(|v| out.push(v.to_string()));
    }
    out
}

pub fn profile(data: &Data, ci: usize) -> Profile {
    let name = &data.cols[ci].name;
    let s = data.df.column(name).expect("col").as_materialized_series();
    let nulls = s.null_count();
    let kind = data.cols[ci].kind;
    let (num, hist) = match kind {
        ColKind::Number => {
            let v = numbers(s);
            if v.is_empty() {
                (None, None)
            } else {
                let (mut mn, mut mx, mut sum, mut sumsq) =
                    (f64::INFINITY, f64::NEG_INFINITY, 0.0, 0.0);
                for &x in &v {
                    mn = mn.min(x);
                    mx = mx.max(x);
                    sum += x;
                    sumsq += x * x;
                }
                let n = v.len() as u64;
                let mean = sum / n as f64;
                let var = (sumsq / n as f64 - mean * mean).max(0.0);
                let std = var.sqrt();
                const NBINS: usize = 48;
                let mut bins = vec![0u64; NBINS];
                for &x in &v {
                    let b = if mx > mn {
                        (((x - mn) / (mx - mn)) * NBINS as f64) as usize
                    } else {
                        0
                    };
                    bins[b.min(NBINS - 1)] += 1;
                }
                let mut sorted = v.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let quantile = |q: f64| -> f64 {
                    if sorted.is_empty() {
                        return 0.0;
                    }
                    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
                    sorted[idx.min(sorted.len() - 1)]
                };
                let p25 = quantile(0.25);
                let p50 = quantile(0.50);
                let p75 = quantile(0.75);
                let p90 = quantile(0.90);
                let p99 = quantile(0.99);
                let iqr = p75 - p25;
                (
                    Some(NumStats {
                        min: mn,
                        max: mx,
                        mean,
                        std,
                        count: n,
                        p25,
                        p50,
                        p75,
                        p90,
                        p99,
                        iqr,
                    }),
                    Some(bins),
                )
            }
        }
        _ => (None, None),
    };
    let (top, distinct) = match kind {
        ColKind::Text | ColKind::Bool => {
            let ss = strings(s);
            let mut counts = std::collections::HashMap::new();
            for v in &ss {
                *counts.entry(v.clone()).or_insert(0u64) += 1;
            }
            let mut items: Vec<(String, u64)> = counts.into_iter().collect();
            let distinct = items.len() as u64;
            items.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            items.truncate(10);
            (Some(items), Some(distinct))
        }
        _ => (None, None),
    };
    Profile { nulls, num, top, distinct, hist }
}

pub fn chart(data: &Data, ci: usize, k: ChartKind, x_col: Option<usize>) -> Option<ChartData> {
    let name = &data.cols[ci].name;
    let s = data.df.column(name).expect("col").as_materialized_series();
    let p = profile(data, ci);
    match data.cols[ci].kind {
        ColKind::Number => match k {
            ChartKind::Hist => p
                .hist
                .zip(p.num)
                .map(|(bins, stats)| ChartData::Hist { bins, stats }),
            ChartKind::Line | ChartKind::Scatter => {
                let yv = numbers(s);
                let n = yv.len();
                if n == 0 {
                    return Some(ChartData::Series { pts: vec![] });
                }
                let mut pts: Vec<(f64, f64)> = if let Some(xi) = x_col {
                    if xi < data.cols.len() && xi != ci {
                        let xs = data.df.column(&data.cols[xi].name).expect("col").as_materialized_series();
                        let xv = numbers(xs);
                        let m = xv.len().min(yv.len());
                        let stride = ((m as f64) / 720.0).ceil() as usize;
                        (0..m).step_by(stride).map(|i| (xv[i], yv[i])).collect()
                    } else {
                        let stride = ((n as f64) / 720.0).ceil() as usize;
                        (0..n).step_by(stride).map(|i| (i as f64, yv[i])).collect()
                    }
                } else {
                    let stride = ((n as f64) / 720.0).ceil() as usize;
                    (0..n).step_by(stride).map(|i| (i as f64, yv[i])).collect()
                };
                if k == ChartKind::Line && x_col.is_some() {
                    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                }
                Some(ChartData::Series { pts })
            }
            ChartKind::TopK => None,
        },
        ColKind::Bool | ColKind::Text => {
            let items = p.top.unwrap_or_default();
            let total = (data.df.height() - p.nulls) as u64;
            Some(ChartData::TopK { items, total })
        }
    }
}

pub fn export(df: &DataFrame, path_str: &str) -> Result<()> {
    let p = std::path::Path::new(path_str);
    let mut file = std::fs::File::create(p)
        .with_context(|| format!("create {}", p.display()))?;
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "parquet" | "pq" => {
            ParquetWriter::new(file).finish(&mut df.clone())?;
        }
        "jsonl" | "ndjson" | "json" => {
            JsonWriter::new(file)
                .with_json_format(JsonFormat::JsonLines)
                .finish(&mut df.clone())?;
        }
        _ => {
            CsvWriter::new(&mut file).finish(&mut df.clone())?;
        }
    }
    Ok(())
}

pub fn filter_data(data: &Data, query: &str) -> Result<DataFrame> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(data.orig_df.clone());
    }
    let ops = [">=", "<=", "!=", "=", ">", "<", ":"];
    for op in ops {
        if let Some(pos) = q.find(op) {
            let col_name = q[..pos].trim();
            let val_part = q[pos + op.len()..].trim();
            if let Some(ci) = data.cols.iter().position(|c| c.name.eq_ignore_ascii_case(col_name)) {
                let s = data.df.column(&data.cols[ci].name)?.as_materialized_series();
                let kind = data.cols[ci].kind;
                let mut matched_indices = Vec::new();
                for r in 0..data.df.height() {
                    let cell_str = cell_string(s, r, kind);
                    let ok = match op {
                        ">" | ">=" | "<" | "<=" => {
                            if let (Ok(cv), Ok(pv)) = (cell_str.parse::<f64>(), val_part.parse::<f64>()) {
                                match op {
                                    ">" => cv > pv,
                                    ">=" => cv >= pv,
                                    "<" => cv < pv,
                                    "<=" => cv <= pv,
                                    _ => false,
                                }
                            } else {
                                false
                            }
                        }
                        "=" | "==" | ":" => cell_str.eq_ignore_ascii_case(val_part.trim_matches('"')),
                        "!=" => !cell_str.eq_ignore_ascii_case(val_part.trim_matches('"')),
                        _ => false,
                    };
                    if ok {
                        matched_indices.push(r as u32);
                    }
                }
                let idx_ca = UInt32Chunked::from_iter_values("idx".into(), matched_indices.into_iter());
                return Ok(data.df.take(&idx_ca)?);
            }
        }
    }

    let needle = q.to_lowercase();
    let hits = find_all(data, &needle);
    let row_set: std::collections::BTreeSet<u32> = hits.into_iter().map(|(r, _)| r as u32).collect();
    let idx_ca = UInt32Chunked::from_iter_values("idx".into(), row_set.into_iter());
    Ok(data.df.take(&idx_ca)?)
}


/// All cells (row-major) whose rendered text contains `q`, case-insensitive.
pub fn find_all(data: &Data, q: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let ncols = data.cols.len();
    let height = data.df.height();
    if ncols == 0 || height == 0 {
        return out;
    }
    let needle = q.to_lowercase();
    let series: Vec<(Series, ColKind)> = data
        .cols
        .iter()
        .map(|c| {
            let s = data
                .df
                .column(c.name.as_str())
                .expect("columns are stable")
                .as_materialized_series();
            (s.clone(), c.kind)
        })
        .collect();
    for r in 0..height {
        for ci in 0..ncols {
            let t = cell_string(&series[ci].0, r, series[ci].1);
            if !t.is_empty() && t.to_lowercase().contains(&needle) {
                out.push((r, ci));
            }
        }
    }
    out
}

pub fn cell_string(s: &Series, row: usize, _kind: ColKind) -> String {    use polars::prelude::AnyValue;
    let out = match s.get(row) {
        Ok(av) => match av {
            AnyValue::Null => String::new(),
            AnyValue::Float64(v)
                if v.abs() >= 1e12 || (v != 0.0 && v.abs() < 1e-4) =>
            {
                format!("{:e}", v)
            }
            AnyValue::Float64(v) => fmt_float(v),
            AnyValue::Float32(v) => fmt_float(v as f64),
            AnyValue::String(v) => v.to_string(),
            other => other.to_string(),
        },
        Err(_) => String::new(),
    };
    out
}

fn fmt_float(v: f64) -> String {
    let mut s = format!("{:.4}", v);
    if s.ends_with('0') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}
