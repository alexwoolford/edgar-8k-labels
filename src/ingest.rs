//! One UTC calendar day of 8-K / 8-K/A labels.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{Days, NaiveDate, Utc};

use crate::db::{upsert_filing, upsert_run, FilingRow, WorkDb};
use crate::http::{filing_url, submissions_url, Fetcher, TICKERS_URL};
use crate::index::{master_index_url, parse_master_index, IndexRow};
use crate::sgml::{join_items, normalize_accession, parse_items};
use crate::submissions;
use crate::tickers::{parse_tickers_json, primary_listings};

#[derive(Debug, Clone, Default)]
pub struct IngestStats {
    pub status: String,
    pub filings_seen: i64,
    pub filings_upserted: i64,
    pub filings_failed: i64,
    pub txt_ok: i64,
    pub submissions_fallback: i64,
    pub index_url: String,
}

pub fn ingest_day(
    db: &mut WorkDb,
    date: NaiveDate,
    fetcher: &mut dyn Fetcher,
) -> Result<IngestStats> {
    let started = Utc::now();
    let as_of = date.format("%Y-%m-%d").to_string();
    let index_url = master_index_url(date);
    let mut stats = IngestStats {
        index_url: index_url.clone(),
        status: "ok".into(),
        ..IngestStats::default()
    };

    let idx_resp = fetcher.get(&index_url).context("GET master index")?;
    if idx_resp.status == 404 {
        stats.status = "ok".into();
        finish(db, &as_of, started, &stats)?;
        return Ok(stats);
    }
    if idx_resp.status != 200 {
        stats.status = "error".into();
        finish(db, &as_of, started, &stats)?;
        anyhow::bail!("master index HTTP {} for {index_url}", idx_resp.status);
    }

    let rows = parse_master_index(&idx_resp.body);
    stats.filings_seen = rows.len() as i64;

    let tickers = if rows.is_empty() {
        HashMap::new()
    } else {
        match load_tickers(fetcher) {
            Ok(t) => t,
            Err(err) => {
                stats.status = "error".into();
                finish(db, &as_of, started, &stats)?;
                return Err(err).context("company_tickers_exchange.json");
            }
        }
    };

    let mut submissions_cache: HashMap<String, String> = HashMap::new();
    let tx = db.unchecked_transaction()?;
    for row in &rows {
        match resolve_filing(
            &tx,
            fetcher,
            row,
            &tickers,
            &mut submissions_cache,
            &mut stats,
        ) {
            Ok(Some(filing)) => {
                if upsert_filing(&tx, &filing)? {
                    stats.filings_upserted += 1;
                }
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(
                    cik = %row.cik,
                    filename = %row.filename,
                    error = %err,
                    "filing failed"
                );
                stats.filings_failed += 1;
            }
        }
    }
    if stats.filings_failed > 0 {
        stats.status = "partial".into();
    }
    upsert_run(
        &tx,
        &as_of,
        started,
        Utc::now(),
        &stats.status,
        &stats.index_url,
        stats.filings_seen,
        stats.filings_upserted,
        stats.filings_failed,
        stats.txt_ok,
        stats.submissions_fallback,
    )?;
    tx.commit()?;
    db.nudge.send();
    Ok(stats)
}

fn finish(
    db: &mut WorkDb,
    as_of: &str,
    started: chrono::DateTime<Utc>,
    stats: &IngestStats,
) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    upsert_run(
        &tx,
        as_of,
        started,
        Utc::now(),
        &stats.status,
        &stats.index_url,
        stats.filings_seen,
        stats.filings_upserted,
        stats.filings_failed,
        stats.txt_ok,
        stats.submissions_fallback,
    )?;
    tx.commit()?;
    db.nudge.send();
    Ok(())
}

fn load_tickers(fetcher: &mut dyn Fetcher) -> Result<HashMap<String, String>> {
    let resp = fetcher
        .get(TICKERS_URL)
        .context("GET company_tickers_exchange.json")?;
    if resp.status != 200 {
        anyhow::bail!("tickers HTTP {}", resp.status);
    }
    let companies = parse_tickers_json(resp.body.as_bytes())?;
    Ok(primary_listings(&companies))
}

fn resolve_filing(
    _tx: &rusqlite::Transaction<'_>,
    fetcher: &mut dyn Fetcher,
    row: &IndexRow,
    tickers: &HashMap<String, String>,
    submissions_cache: &mut HashMap<String, String>,
    stats: &mut IngestStats,
) -> Result<Option<FilingRow>> {
    let accession = accession_from_filename(&row.filename);
    let url = filing_url(&row.filename);
    let txt = fetcher.get(&url)?;
    let mut filing = FilingRow::from_index(row, accession.clone());
    filing.ticker = tickers.get(&row.cik).cloned();

    if txt.status == 200 {
        let items = parse_items(&txt.body);
        if !items.is_empty() {
            filing.items = join_items(&items);
            filing.source = "txt".into();
            stats.txt_ok += 1;
            return Ok(Some(filing));
        }
    } else if txt.status != 403 && txt.status != 404 {
        anyhow::bail!("filing HTTP {} for {url}", txt.status);
    }

    match submissions_items(fetcher, submissions_cache, &row.cik, &accession)? {
        Some(items) => {
            filing.items = items;
            filing.source = "submissions_json".into();
            stats.submissions_fallback += 1;
            Ok(Some(filing))
        }
        None if txt.status == 200 => {
            // Header had no `<ITEMS>` / mappable ITEM INFORMATION; recent JSON
            // does not list this accession. Keep the filing, do not guess codes.
            filing.items = None;
            filing.source = "txt".into();
            stats.txt_ok += 1;
            Ok(Some(filing))
        }
        None => anyhow::bail!("accession {accession} not in submissions recent"),
    }
}

fn submissions_items(
    fetcher: &mut dyn Fetcher,
    cache: &mut HashMap<String, String>,
    cik: &str,
    accession: &str,
) -> Result<Option<Option<String>>> {
    let json = if let Some(cached) = cache.get(cik) {
        cached.clone()
    } else {
        let sub_url = submissions_url(cik);
        let resp = fetcher.get(&sub_url)?;
        if resp.status != 200 {
            anyhow::bail!("submissions HTTP {} for {sub_url}", resp.status);
        }
        cache.insert(cik.to_string(), resp.body.clone());
        resp.body
    };
    match submissions::items_for_accession(&json, accession)? {
        submissions::AccessionItems::Present(items) => Ok(Some(items)),
        submissions::AccessionItems::Missing => Ok(None),
    }
}

fn accession_from_filename(filename: &str) -> String {
    let base = filename.rsplit('/').next().unwrap_or(filename);
    let stem = base.strip_suffix(".txt").unwrap_or(base);
    normalize_accession(stem)
}

/// Parse `YYYY-MM-DD`. Used by the CLI.
pub fn parse_as_of(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").with_context(|| format!("date {s}"))
}

pub fn default_as_of() -> NaiveDate {
    Utc::now()
        .date_naive()
        .checked_sub_days(Days::new(1))
        .expect("yesterday")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{last_run, lookup_filings, open_work, outbox_count};
    use crate::http::{HttpResponse, MapFetcher};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV: Mutex<()> = Mutex::new(());

    struct TestDb {
        _lock: std::sync::MutexGuard<'static, ()>,
        _dir: TempDir,
        db: WorkDb,
    }

    fn test_db() -> TestDb {
        let lock = ENV.lock().unwrap_or_else(|p| p.into_inner());
        let dir = TempDir::new().unwrap();
        let announce = dir.path().join("announce");
        std::fs::create_dir_all(&announce).unwrap();
        std::env::set_var("STATE_CAPTURE_ANNOUNCE_DIR", announce.to_str().unwrap());
        let db = open_work(&dir.path().join("edgar-8k-labels.sqlite")).unwrap();
        TestDb {
            _lock: lock,
            _dir: dir,
            db,
        }
    }

    fn fixture_fetcher() -> MapFetcher {
        let date = NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        let idx = master_index_url(date);
        let mut urls = HashMap::new();
        urls.insert(
            idx,
            HttpResponse {
                status: 200,
                body: include_str!("../fixtures/master.idx").into(),
            },
        );
        urls.insert(
            TICKERS_URL.to_string(),
            HttpResponse {
                status: 200,
                body: include_str!("../fixtures/company_tickers_exchange.json").into(),
            },
        );
        urls.insert(
            filing_url("edgar/data/320193/0000320193-26-000123.txt"),
            HttpResponse {
                status: 200,
                body: include_str!("../fixtures/aapl-8k.txt").into(),
            },
        );
        urls.insert(
            filing_url("edgar/data/104169/0000104169-26-000050.txt"),
            HttpResponse {
                status: 403,
                body: "forbidden".into(),
            },
        );
        urls.insert(
            submissions_url("0000104169"),
            HttpResponse {
                status: 200,
                body: include_str!("../fixtures/submissions-wmt.json").into(),
            },
        );
        MapFetcher { urls }
    }

    #[test]
    fn weekend_404_is_ok_zero_filings() {
        let mut t = test_db();
        let date = NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
        let url = master_index_url(date);
        let mut fetcher = MapFetcher {
            urls: HashMap::from([(
                url,
                HttpResponse {
                    status: 404,
                    body: "not found".into(),
                },
            )]),
        };
        let stats = ingest_day(&mut t.db, date, &mut fetcher).unwrap();
        assert_eq!(stats.status, "ok");
        assert_eq!(stats.filings_seen, 0);
        let run = last_run(&t.db).unwrap().unwrap();
        assert_eq!(run.status, "ok");
        assert_eq!(run.as_of_date, "2026-09-12");
    }

    #[test]
    fn ingest_fixture_and_unchanged_rerun_no_extra_outbox() {
        let mut t = test_db();
        let date = NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        let mut fetcher = fixture_fetcher();
        let stats = ingest_day(&mut t.db, date, &mut fetcher).unwrap();
        assert_eq!(stats.filings_seen, 2);
        assert_eq!(stats.txt_ok, 1);
        assert_eq!(stats.submissions_fallback, 1);
        assert_eq!(stats.filings_failed, 0);
        assert_eq!(stats.status, "ok");
        let n1 = outbox_count(&t.db).unwrap();
        assert!(n1 >= 3, "2 filings + 1 run, got {n1}");

        let aapl = lookup_filings(&t.db, "AAPL").unwrap();
        assert_eq!(aapl.len(), 1);
        assert_eq!(aapl[0].items.as_deref(), Some("2.01,9.01"));
        assert_eq!(aapl[0].source, "txt");
        assert_eq!(aapl[0].ticker.as_deref(), Some("AAPL"));

        let wmt = lookup_filings(&t.db, "0000104169").unwrap();
        assert_eq!(wmt[0].source, "submissions_json");
        assert_eq!(wmt[0].is_amendment, 1);

        let mut fetcher = fixture_fetcher();
        ingest_day(&mut t.db, date, &mut fetcher).unwrap();
        let n2 = outbox_count(&t.db).unwrap();
        // run row always rewrites (one extra U); filings must not.
        assert_eq!(n2, n1 + 1, "unchanged filings must not emit extra outbox");
    }

    #[test]
    fn rolled_back_tx_zero_outbox() {
        let t = test_db();
        let before = outbox_count(&t.db).unwrap();
        {
            let tx = t.db.unchecked_transaction().unwrap();
            let f = FilingRow {
                accession: "0000000000-26-000001".into(),
                cik: "0000000000".into(),
                ticker: None,
                company_name: "X".into(),
                form: "8-K".into(),
                is_amendment: 0,
                filed_date: "2026-09-11".into(),
                items: None,
                filename: "edgar/data/0/x.txt".into(),
                source: "txt".into(),
            };
            upsert_filing(&tx, &f).unwrap();
            tx.rollback().unwrap();
        }
        assert_eq!(outbox_count(&t.db).unwrap(), before);
    }

    #[test]
    fn capture_installs_outbox() {
        let t = test_db();
        let n: i64 =
            t.db.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_outbox'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn source_has_no_insert_or_replace() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        fn walk(p: &std::path::Path, hits: &mut Vec<String>) {
            for e in std::fs::read_dir(p).unwrap() {
                let e = e.unwrap();
                let path = e.path();
                if path.is_dir() {
                    walk(&path, hits);
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                    continue;
                }
                let body = std::fs::read_to_string(&path).unwrap();
                let needle = format!("{}{}{}", "OR ", "REPLACE ", "INTO");
                if body.to_ascii_uppercase().contains(&needle) {
                    hits.push(path.display().to_string());
                }
            }
        }
        let mut hits = Vec::new();
        walk(&root, &mut hits);
        assert!(hits.is_empty(), "forbidden replace idiom in {hits:?}");
    }
}
