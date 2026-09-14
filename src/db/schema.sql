CREATE TABLE IF NOT EXISTS filings (
    accession TEXT PRIMARY KEY,
    cik TEXT NOT NULL,
    ticker TEXT,
    company_name TEXT NOT NULL,
    form TEXT NOT NULL,
    is_amendment INTEGER NOT NULL CHECK (is_amendment IN (0, 1)),
    filed_date TEXT NOT NULL,
    items TEXT,
    filename TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('txt', 'submissions_json')),
    deleted_at INTEGER
) STRICT;

CREATE TABLE IF NOT EXISTS ingest_runs (
    as_of_date TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ok', 'partial', 'error')),
    index_url TEXT NOT NULL,
    filings_seen INTEGER NOT NULL,
    filings_upserted INTEGER NOT NULL,
    filings_failed INTEGER NOT NULL,
    txt_ok INTEGER NOT NULL,
    submissions_fallback INTEGER NOT NULL
) STRICT;
