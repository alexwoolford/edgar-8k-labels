//! Nightly EDGAR 8-K / 8-K/A item labels. A label spine, not a leading observable.

pub mod db;
pub mod http;
pub mod index;
pub mod ingest;
pub mod sec_ua;
pub mod sgml;
pub mod submissions;
pub mod tickers;
pub mod time;

pub use db::{open, open_work, WorkDb, DB_NAME};
pub use ingest::{ingest_dates, ingest_day, ingest_range, IngestStats};
pub use sec_ua::{validate_user_agent, UserAgentError};
pub use time::{utc_date, utc_iso, DATE_FMT, INSTANT_FMT};
