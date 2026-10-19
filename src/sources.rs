use anyhow::{anyhow, bail, Context, Result};
use polars::prelude::*;

use crate::data::{build, Data};

// ---------- shared cell model ----------

#[derive(Debug, Clone)]
pub enum Av {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

#[derive(Clone, Copy)]
enum ColT {
    Int,
    Float,
    Bool,
    Str,
}

/// Build a `Data` from raw row tuples with per-column type inference.
pub fn frame_from_cells(display: String, cols: Vec<String>, rows: Vec<Vec<Av>>) -> Result<Data> {
    if cols.is_empty() {
        bail!("no columns found in {}", display);
    }
    let mut kinds: Vec<ColT> = Vec::with_capacity(cols.len());
    for ci in 0..cols.len() {
        let mut has_int = false;
        let mut has_float = false;
        let mut has_bool = false;
        let mut has_str = false;
        for r in &rows {
            match r.get(ci) {
                None | Some(Av::Null) => {}
                Some(Av::Int(_)) => has_int = true,
                Some(Av::Float(_)) => has_float = true,
                Some(Av::Bool(_)) => has_bool = true,
                Some(Av::Str(_)) => has_str = true,
            }
        }
        let k = if has_str {
            ColT::Str
        } else if has_int && has_float {
            ColT::Float
        } else if has_float {
            ColT::Float
        } else if has_int {
            ColT::Int
        } else if has_bool {
            ColT::Bool
        } else {
            ColT::Str // all-null column
        };
        kinds.push(k);
    }
    let df = to_df(&cols, &kinds, rows).context("building dataframe")?;
    build(df, false, display)
}

fn to_df(cols: &[String], kinds: &[ColT], rows: Vec<Vec<Av>>) -> Result<DataFrame> {
    let mut series = Vec::with_capacity(cols.len());
    for (ci, name) in cols.iter().enumerate() {
        let s: Series = match kinds[ci] {
            ColT::Int => {
                let v: Vec<Option<i64>> = rows
                    .iter()
                    .map(|r| match r.get(ci) {
                        Some(Av::Int(i)) => Some(*i),
                        Some(Av::Float(f)) if f.fract() == 0.0 && f.is_finite() => Some(*f as i64),
                        _ => None,
                    })
                    .collect();
                Series::new(PlSmallStr::from(name.as_str()), v)
            }
            ColT::Float => {
                let v: Vec<Option<f64>> = rows
                    .iter()
                    .map(|r| match r.get(ci) {
                        Some(Av::Int(i)) => Some(*i as f64),
                        Some(Av::Float(f)) => Some(*f),
                        _ => None,
                    })
                    .collect();
                Series::new(PlSmallStr::from(name.as_str()), v)
            }
            ColT::Bool => {
                let v: Vec<Option<bool>> = rows
                    .iter()
                    .map(|r| match r.get(ci) {
                        Some(Av::Bool(b)) => Some(*b),
                        Some(Av::Int(i)) => Some(*i != 0),
                        _ => None,
                    })
                    .collect();
                Series::new(PlSmallStr::from(name.as_str()), v)
            }
            ColT::Str => {
                let v: Vec<Option<String>> = rows
                    .iter()
                    .map(|r| match r.get(ci) {
                        Some(Av::Bool(b)) => Some(b.to_string()),
                        Some(Av::Int(i)) => Some(i.to_string()),
                        Some(Av::Float(f)) => Some(fmt_any(*f)),
                        Some(Av::Str(s)) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                Series::new(PlSmallStr::from(name.as_str()), v)
            }
        };
        series.push(s.into_column());
    }
    DataFrame::new(series).map_err(|e| anyhow!("{e}"))
}

pub fn fmt_any(f: f64) -> String {
    if f.abs() >= 1e12 || (f != 0.0 && f.abs() < 1e-6) {
        format!("{:e}", f)
    } else {
        let s = format!("{:.6}", f);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn hex_str(b: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(2 + b.len() * 2);
    let _ = write!(s, "0x");
    for byte in b {
        let _ = write!(s, "{:02x}", byte);
    }
    s
}

// ---------- excel ----------

pub fn load_excel(path: &std::path::Path) -> Result<Data> {
    use calamine::Reader as _;
    let mut wb = calamine::open_workbook_auto(path)?;
    let sheet = wb
        .sheet_names()
        .first()
        .cloned()
        .with_context(|| format!("no sheets in {}", path.display()))?;
    let display = format!("{}::{}", path.display(), sheet);
    eprintln!("loading {} …", display);
    let range = wb
        .worksheet_range(&sheet)
        .with_context(|| format!("read sheet {}", display))?;
    let mut headers: Vec<String> = Vec::new();
    let mut iter = range.rows();
    if let Some(first) = iter.next() {
        let mut used = std::collections::HashSet::new();
        for (i, cell) in first.iter().enumerate() {
            use calamine::DataType as _;
            let mut h = if cell.is_empty() {
                String::new()
            } else if let Some(s) = cell.as_string() {
                s
            } else if let Some(i) = cell.as_i64() {
                i.to_string()
            } else if let Some(f) = cell.as_f64() {
                fmt_any(f)
            } else {
                String::new()
            };
            h = h.trim().to_string();
            if h.is_empty() {
                h = format!("col{}", i + 1);
            }
            let mut final_name = h.clone();
            let mut k = 2;
            while !used.insert(final_name.clone()) {
                final_name = format!("{}{}", h, k);
                k += 1;
            }
            headers.push(final_name);
        }
    } else {
        bail!("empty sheet {}", display);
    }
    let mut rows: Vec<Vec<Av>> = Vec::new();
    for row in iter {
        use calamine::DataType as _;
        let mut cells = Vec::with_capacity(row.len());
        for cell in row {
            let av = if cell.is_empty() {
                Av::Null
            } else if cell.is_bool() {
                Av::Bool(cell.get_bool().unwrap_or(false))
            } else if cell.is_datetime() {
                match cell.as_datetime() {
                    Some(d) => Av::Str(d.format("%Y-%m-%d %H:%M:%S").to_string()),
                    None => Av::Str(fmt_any(cell.as_f64().unwrap_or(0.0))),
                }
            } else if let Some(i) = cell.as_i64() {
                Av::Int(i)
            } else if let Some(f) = cell.as_f64() {
                Av::Float(f)
            } else if let Some(s) = cell.as_string() {
                Av::Str(s)
            } else {
                Av::Null
            };
            cells.push(av);
        }
        rows.push(cells);
    }
    frame_from_cells(display, headers, rows)
}

// ---------- sqlite ----------

pub fn load_sqlite(path: &std::path::Path, table_hint: Option<String>) -> Result<Data> {
    use rusqlite::{types::ValueRef, Connection};
    let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .context("list tables")?;
        stmt.query_map([], |r| r.get::<_, String>(0))
            .context("list tables")?
            .filter_map(|r| r.ok())
            .collect()
    };
    if tables.is_empty() {
        bail!("no tables in {}", path.display());
    }
    let table = pick_table(table_hint, tables, |t| {
        conn.query_row(&format!("SELECT count(*) FROM \"{}\"", esc(t)), [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|e| anyhow!("{e}"))
    });
    let table = match table? {
        Ok(t) => t,
        Err(candidates) => bail!("{}: table not found; tables: {}", path.display(), candidates),
    };
    eprintln!("loading {}::{} …", path.display(), table);
    let sql = format!("SELECT * FROM \"{}\"", esc(&table));
    let mut stmt = conn.prepare(&sql).with_context(|| sql.clone())?;
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows: Vec<Vec<Av>> = Vec::new();
    let mut results = stmt.query([])?;
    while let Some(row) = results.next()? {
        let mut cells = Vec::with_capacity(col_names.len());
        for i in 0..col_names.len() {
            cells.push(match row.get_ref(i)? {
                ValueRef::Null => Av::Null,
                ValueRef::Integer(v) => Av::Int(v),
                ValueRef::Real(v) => Av::Float(v),
                ValueRef::Text(t) => Av::Str(String::from_utf8_lossy(t).into_owned()),
                ValueRef::Blob(b) => Av::Str(if b.len() <= 32 {
                    hex_str(b)
                } else {
                    format!("<blob {} bytes>", b.len())
                }),
            });
        }
        rows.push(cells);
    }
    let display = format!("{}::{}", path.display(), table);
    frame_from_cells(display, col_names, rows)
}

// ---------- duckdb ----------

pub fn load_duckdb(path: &std::path::Path, table_hint: Option<String>) -> Result<Data> {
    let conn = duckdb::Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT table_name FROM information_schema.tables WHERE table_schema='main' ORDER BY table_name")
            .context("list tables")?;
        stmt.query_map([], |r| r.get::<_, String>(0))
            .context("list tables")?
            .filter_map(|r| r.ok())
            .collect()
    };
    if tables.is_empty() {
        bail!("no tables in {}", path.display());
    }
    let picked = pick_table(table_hint, tables, |t| {
        conn.query_row(
            &format!("SELECT count(*) FROM \"{}\"", esc(t)),
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| anyhow!("{e}"))
    });
    let table = match picked {
        Ok(Ok(t)) => t,
        Ok(Err(candidates)) => bail!("{}: table not found; tables: {}", path.display(), candidates),
        Err(e) => return Err(e),
    };
    eprintln!("loading {}::{} …", path.display(), table);
    // DESCRIBE gives (column_name, column_type, …); use it to cast awkward
    // types (decimals, timestamps, blobs…) to plain doubles/strings in SQL,
    // since the crate's private deps make those variants hard to decode.
    let cols: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(&format!("DESCRIBE SELECT * FROM \"{}\"", esc(&table)))
            .context("describe table")?;
        stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect()
    };
    if cols.is_empty() {
        bail!("{}::{}: no columns", path.display(), table);
    }
    let select_exprs: Vec<String> = cols
        .iter()
        .map(|(name, ty)| {
            let up = ty.to_uppercase();
            let cast = if up.starts_with("DECIMAL") {
                "::DOUBLE"
            } else if up.starts_with("TIMESTAMP")
                || up.starts_with("DATE")
                || up.starts_with("TIME")
                || up.starts_with("INTERVAL")
                || up.starts_with("UUID")
                || up.starts_with("BLOB")
            {
                "::VARCHAR"
            } else {
                ""
            };
            if cast.is_empty() {
                format!("\"{}\"", name)
            } else {
                format!("\"{}\"{} AS \"{}\"", name, cast, name)
            }
        })
        .collect();
    let sql = format!("SELECT {} FROM \"{}\"", select_exprs.join(", "), esc(&table));
    let mut stmt = conn.prepare(&sql).with_context(|| sql.clone())?;
    let col_names: Vec<String> = cols.iter().map(|(n, _)| n.clone()).collect();
    use duckdb::types::ValueRef;
    let mut rows: Vec<Vec<Av>> = Vec::new();
    {
        let mut results = stmt.query([])?;
        while let Ok(Some(row)) = results.next() {
            let mut cells = Vec::with_capacity(col_names.len());
            for i in 0..col_names.len() {
                cells.push(match row.get_ref(i)? {
                    ValueRef::Null => Av::Null,
                    ValueRef::Boolean(v) => Av::Bool(v),
                    ValueRef::TinyInt(v) => Av::Int(v as i64),
                    ValueRef::SmallInt(v) => Av::Int(v as i64),
                    ValueRef::Int(v) => Av::Int(v as i64),
                    ValueRef::BigInt(v) => Av::Int(v),
                    ValueRef::HugeInt(v) => Av::Float(v as f64),
                    ValueRef::UTinyInt(v) => Av::Int(v as i64),
                    ValueRef::USmallInt(v) => Av::Int(v as i64),
                    ValueRef::UInt(v) => Av::Int(v as i64),
                    ValueRef::UBigInt(v) => Av::Float(v as f64),
                    ValueRef::Float(v) => Av::Float(v as f64),
                    ValueRef::Double(v) => Av::Float(v),
                    ValueRef::Text(t) => Av::Str(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Blob(b) => Av::Str(hex_str(b)),
                    other => Av::Str(format!("{:?}", other)),
                });
            }
            rows.push(cells);
        }
    }
    let display = format!("{}::{}", path.display(), table);
    frame_from_cells(display, col_names, rows)
}

// ---------- postgres ----------

pub fn load_postgres(uri: &str) -> Result<Data> {
    use postgres::{Client, NoTls};
    let (connstr, table_hint) = match uri.rsplit_once("::") {
        Some((c, t)) if !c.is_empty() && !t.is_empty() => (c.to_string(), Some(t.to_string())),
        _ => (uri.to_string(), None),
    };
    let mut client =
        Client::connect(&connstr, NoTls).context("connect (postgres://user:pass@host/database)")?;
    let server: String = match client.query_one("SELECT current_database()", &[]) {
        Ok(row) => row.get::<_, String>(0),
        Err(_) => "pg".to_string(),
    };
    let mut tables: Vec<(String, i64)> = client
        .query(
            "SELECT relname::text, COALESCE(n_live_tup, 0)::bigint AS n
             FROM pg_stat_user_tables ORDER BY n DESC, relname LIMIT 200",
            &[],
        )?
        .iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, i64>(1)))
        .collect();
    if tables.is_empty() {
        tables = client
            .query(
                "SELECT table_name::text, 0::bigint FROM information_schema.tables
                 WHERE table_schema='public' ORDER BY table_name LIMIT 200",
                &[],
            )?
            .iter()
            .map(|r| (r.get::<_, String>(0), r.get::<_, i64>(1)))
            .collect();
    }
    if tables.is_empty() {
        bail!("postgres: no tables visible");
    }
    let table = match pick_table(table_hint, tables.iter().map(|(n, _)| n.clone()).collect(), |t| {
        let n: i64 = client
            .query_one(
                &format!("SELECT count(*) FROM \"{}\"", esc(t)),
                &[],
            )
            .map_err(|e| anyhow!("{e}"))?
            .get::<_, i64>(0);
        Ok(n)
    }) {
        Ok(Ok(t)) => t,
        Ok(Err(cands)) => bail!("postgres: table not found; tables: {}", cands),
        Err(e) => return Err(e),
    };
    let table = table;
    eprintln!("loading {}{} (…) …", server, table);
    // column type plan
    let plan: Vec<(String, String)> = client
        .query(
            "SELECT column_name::text, data_type::text FROM information_schema.columns
             WHERE table_name = $1 ORDER BY ordinal_position",
            &[&table],
        )?
        .iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
        .collect();
    if plan.is_empty() {
        bail!("postgres: table {} not found", table);
    }
    let kind = |dt: &str| -> u8 {
        match dt {
            "smallint" | "integer" | "bigint" => b'i',
            "real" | "double precision" | "numeric" | "decimal" | "money" => b'f',
            "boolean" => b'b',
            _ => b's',
        }
    };
    let mut sels = Vec::with_capacity(plan.len());
    for (name, dt) in &plan {
        match kind(dt) {
            b'i' => sels.push(format!("\"{}\"", name)),
            b'b' => sels.push(format!("\"{}\"", name)),
            b'f' => sels.push(format!("\"{}\"::double precision AS \"{}\"", name, name)),
            _ => sels.push(format!("\"{}\"::text AS \"{}\"", name, name)),
        }
    }
    let sql = format!("SELECT {} FROM \"{}\"", sels.join(", "), esc(&table));
    let rows_all = client.query(&sql, &[]).context("read table")?;
    let mut rows: Vec<Vec<Av>> = Vec::with_capacity(rows_all.len());
    for row in &rows_all {
        let mut cells = Vec::with_capacity(plan.len());
        for (i, (_, dt)) in plan.iter().enumerate() {
            cells.push(match kind(dt) {
                b'i' => row
                    .try_get::<usize, Option<i64>>(i)
                    .map_err(|e| anyhow!("{e}"))?
                    .map(Av::Int)
                    .unwrap_or(Av::Null),
                b'f' => row
                    .try_get::<usize, Option<f64>>(i)
                    .map_err(|e| anyhow!("{e}"))?
                    .map(Av::Float)
                    .unwrap_or(Av::Null),
                b'b' => row
                    .try_get::<usize, Option<bool>>(i)
                    .map_err(|e| anyhow!("{e}"))?
                    .map(Av::Bool)
                    .unwrap_or(Av::Null),
                _ => row
                    .try_get::<usize, Option<String>>(i)
                    .map_err(|e| anyhow!("{e}"))?
                    .map(Av::Str)
                    .unwrap_or(Av::Null),
            });
        }
        rows.push(cells);
    }
    let cols = plan.into_iter().map(|(n, _)| n).collect();
    let display = format!("pg:{}::{}", server, table);
    frame_from_cells(display, cols, rows)
}

// ---------- table picker ----------

fn esc(name: &str) -> String {
    name.replace('"', "\"\"")
}

/// Resolve an optional table hint into a validated name, else pick the biggest table.
/// Returns Ok(Ok(name)) on success, Ok(Err(candidates)) if hint not found,
/// Err(e) on IO/rescuer error.
fn pick_table(
    hint: Option<String>,
    tables: Vec<String>,
    count: impl FnMut(&str) -> anyhow::Result<i64>,
) -> Result<std::result::Result<String, String>> {
    let mut count = count;
    if let Some(h) = hint {
        if tables.iter().any(|t| t == &h) {
            return Ok(Ok(h));
        }
        return Ok(Err(tables.join(", ")));
    }
    let mut best: Option<(i64, &String)> = None;
    for t in &tables {
        match count(t) {
            Ok(n) => {
                if best.map_or(true, |(bn, _)| n > bn) {
                    best = Some((n, t));
                }
            }
            Err(_) => continue,
        }
    }
    Ok(Ok(best
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| tables[0].clone())))
}

pub fn split_table_hint(p: &std::path::Path) -> (std::path::PathBuf, Option<String>) {
    let s = p.to_string_lossy();
    if let Some((file, hint)) = s.rsplit_once("::") {
        if !file.is_empty() && !hint.is_empty() && std::path::Path::new(file).exists() {
            return (std::path::PathBuf::from(file), Some(hint.to_string()));
        }
    }
    (p.to_path_buf(), None)
}

pub fn uri_is_postgres(arg: &std::path::Path) -> Option<String> {
    let s = arg.to_string_lossy().into_owned();
    if s.starts_with("postgres://") || s.starts_with("postgresql://") {
        Some(s)
    } else {
        None
    }
}
