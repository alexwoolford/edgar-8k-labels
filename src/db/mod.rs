use std::ops::{Deref, DerefMut};
use std::path::Path;

use anyhow::{Context, Result};
use capturable_state::{
    apply_runtime_pragmas, install, CaptureConfig, CaptureMode, Nudge, TableSpec,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::index::IndexRow;
use crate::time::utc_iso;

pub const DB_NAME: &str = "edgar-8k-labels";

pub struct WorkDb {
    conn: Connection,
    pub nudge: Nudge,
}

impl Deref for WorkDb {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}

impl DerefMut for WorkDb {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
    }
    let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    apply_schema(&conn)?;
    Ok(conn)
}

pub fn open_work(path: &Path) -> Result<WorkDb> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
    }
    let conn = Connection::open(path).with_context(|| format!("open work {}", path.display()))?;
    apply_runtime_pragmas(&conn)?;
    apply_schema(&conn)?;
    let nudge = install_capture(&conn, path)?;
    Ok(WorkDb { conn, nudge })
}

fn apply_schema(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.execute_batch(include_str!("schema.sql"))
        .context("apply schema")?;
    Ok(())
}

fn install_capture(conn: &Connection, path: &Path) -> Result<Nudge> {
    let tables = [
        TableSpec::new("filings", CaptureMode::After),
        TableSpec::new("ingest_runs", CaptureMode::After),
    ];
    install(conn, &CaptureConfig::new(DB_NAME, path, &tables))
}

#[derive(Debug, Clone)]
pub struct FilingRow {
    pub accession: String,
    pub cik: String,
    pub ticker: Option<String>,
    pub company_name: String,
    pub form: String,
    pub is_amendment: i64,
    pub filed_date: String,
    pub items: Option<String>,
    pub filename: String,
    pub source: String,
}

impl FilingRow {
    pub fn from_index(row: &IndexRow, accession: String) -> Self {
        let is_amendment = i64::from(row.form.eq_ignore_ascii_case("8-K/A"));
        Self {
            accession,
            cik: row.cik.clone(),
            ticker: None,
            company_name: row.company_name.clone(),
            form: row.form.clone(),
            is_amendment,
            filed_date: row.filed_date.clone(),
            items: None,
            filename: row.filename.clone(),
            source: "txt".into(),
        }
    }
}

/// Change-aware upsert. Identical reruns emit no extra `_outbox` row.
pub fn upsert_filing(conn: &Connection, f: &FilingRow) -> Result<bool> {
    let n = conn.execute(
        "INSERT INTO filings (
            accession, cik, ticker, company_name, form, is_amendment,
            filed_date, items, filename, source, deleted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)
         ON CONFLICT(accession) DO UPDATE SET
            cik = excluded.cik,
            ticker = excluded.ticker,
            company_name = excluded.company_name,
            form = excluded.form,
            is_amendment = excluded.is_amendment,
            filed_date = excluded.filed_date,
            items = excluded.items,
            filename = excluded.filename,
            source = excluded.source
         WHERE filings.cik IS NOT excluded.cik
            OR filings.ticker IS NOT excluded.ticker
            OR filings.company_name IS NOT excluded.company_name
            OR filings.form IS NOT excluded.form
            OR filings.is_amendment IS NOT excluded.is_amendment
            OR filings.filed_date IS NOT excluded.filed_date
            OR filings.items IS NOT excluded.items
            OR filings.filename IS NOT excluded.filename
            OR filings.source IS NOT excluded.source",
        params![
            f.accession,
            f.cik,
            f.ticker.as_deref(),
            f.company_name,
            f.form,
            f.is_amendment,
            f.filed_date,
            f.items.as_deref(),
            f.filename,
            f.source,
        ],
    )?;
    Ok(n > 0)
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_run(
    conn: &Connection,
    as_of_date: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
    status: &str,
    index_url: &str,
    seen: i64,
    upserted: i64,
    failed: i64,
    txt_ok: i64,
    submissions_fallback: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO ingest_runs (
            as_of_date, started_at, finished_at, status, index_url,
            filings_seen, filings_upserted, filings_failed, txt_ok, submissions_fallback
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(as_of_date) DO UPDATE SET
            started_at = excluded.started_at,
            finished_at = excluded.finished_at,
            status = excluded.status,
            index_url = excluded.index_url,
            filings_seen = excluded.filings_seen,
            filings_upserted = excluded.filings_upserted,
            filings_failed = excluded.filings_failed,
            txt_ok = excluded.txt_ok,
            submissions_fallback = excluded.submissions_fallback",
        params![
            as_of_date,
            utc_iso(started_at),
            utc_iso(finished_at),
            status,
            index_url,
            seen,
            upserted,
            failed,
            txt_ok,
            submissions_fallback,
        ],
    )?;
    Ok(())
}

#[derive(Debug)]
pub struct StatusRow {
    pub as_of_date: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,
    pub filings_seen: i64,
    pub filings_upserted: i64,
    pub filings_failed: i64,
}

pub fn last_run(conn: &Connection) -> Result<Option<StatusRow>> {
    conn.query_row(
        "SELECT as_of_date, started_at, finished_at, status,
                filings_seen, filings_upserted, filings_failed
         FROM ingest_runs
         ORDER BY as_of_date DESC
         LIMIT 1",
        [],
        |r| {
            Ok(StatusRow {
                as_of_date: r.get(0)?,
                started_at: r.get(1)?,
                finished_at: r.get(2)?,
                status: r.get(3)?,
                filings_seen: r.get(4)?,
                filings_upserted: r.get(5)?,
                filings_failed: r.get(6)?,
            })
        },
    )
    .optional()
    .context("last ingest_runs")
}

pub fn lookup_filings(conn: &Connection, q: &str) -> Result<Vec<FilingRow>> {
    let q = q.trim();
    let cik = crate::tickers::pad_cik(q);
    let acc = crate::sgml::normalize_accession(q);
    let ticker = q.to_ascii_uppercase();
    let mut stmt = conn.prepare(
        "SELECT accession, cik, ticker, company_name, form, is_amendment,
                filed_date, items, filename, source
         FROM filings
         WHERE accession = ?1 OR cik = ?2 OR ticker = ?3
         ORDER BY filed_date DESC, accession",
    )?;
    let rows = stmt.query_map(params![acc, cik, ticker], |r| {
        Ok(FilingRow {
            accession: r.get(0)?,
            cik: r.get(1)?,
            ticker: r.get(2)?,
            company_name: r.get(3)?,
            form: r.get(4)?,
            is_amendment: r.get(5)?,
            filed_date: r.get(6)?,
            items: r.get(7)?,
            filename: r.get(8)?,
            source: r.get(9)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn outbox_count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM _outbox", [], |r| r.get(0))
        .context("outbox count")
}
