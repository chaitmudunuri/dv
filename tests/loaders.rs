use dv::data::{self, ColKind};

/// Loader smoke test: every supported format loads a sane frame via data::load.
#[test]
fn all_sample_formats_load() {
    // (file, expected rows, expected cols, first numeric col index)
    let cases: &[(&str, usize, usize, usize)] = &[
        ("samples/demo.csv", 1200, 5, 0),
        ("samples/demo.tsv", 1200, 5, 0),
        ("samples/demo.parquet", 1200, 5, 0),
        ("samples/demo.arrow", 1200, 5, 0),
        ("samples/demo.jsonl", 1200, 5, 0),
        ("samples/demo.avro", 1200, 5, 0),
        ("samples/demo.xlsx", 60, 5, 0),
        ("samples/demo.xlsx::age", 60, 5, 0),
        ("samples/demo.db", 50, 5, 0),       // auto-picks biggest table (events)
        ("samples/demo.db::small", 3, 1, 0),
        ("samples/demo.duckdb", 1000, 2, 0), // auto-picks biggest table (bignum)
        ("samples/demo.duckdb::tips", 4, 3, 0),
        ("samples/demo2.duckdb", 5, 4, 0),   // decimal/timestamp casts
        ("samples/demo.csv.gz", 1200, 5, 0),
        ("samples/demo.tsv.gz", 1200, 5, 0),
        ("samples/demo.jsonl.gz", 1200, 5, 0),
    ];
    let _ = ColKind::Number;
    for (path, rows, cols, _) in cases {
        let d = data::load(std::path::Path::new(path))
            .unwrap_or_else(|e| panic!("load {path}: {e}"));
        assert_eq!(d.df.height(), *rows, "{path} rows");
        assert_eq!(d.cols.len(), *cols, "{path} cols");
    }
}

#[test]
fn unknown_file_fails_cleanly() {
    let d = data::load(std::path::Path::new("samples/definitely-missing.csv"));
    assert!(d.is_err(), "missing file is an error");
}
