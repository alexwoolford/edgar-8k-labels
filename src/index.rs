//! EDGAR daily master index (`master.YYYYMMDD.idx`).

use chrono::{Datelike, NaiveDate};

use crate::tickers::pad_cik;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    pub cik: String,
    pub company_name: String,
    pub form: String,
    pub filed_date: String,
    pub filename: String,
}

pub fn master_index_url(date: NaiveDate) -> String {
    let year = date.year();
    let qtr = (date.month() - 1) / 3 + 1;
    let ymd = date.format("%Y%m%d");
    format!("https://www.sec.gov/Archives/edgar/daily-index/{year}/QTR{qtr}/master.{ymd}.idx")
}

pub fn is_eight_k(form: &str) -> bool {
    let f = form.trim();
    f.eq_ignore_ascii_case("8-K") || f.eq_ignore_ascii_case("8-K/A")
}

pub fn parse_master_index(body: &str) -> Vec<IndexRow> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in body.lines() {
        if line.starts_with("CIK|") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if line.chars().all(|c| c == '-' || c.is_whitespace()) {
            continue;
        }
        let Some(row) = parse_index_line(line) else {
            continue;
        };
        if is_eight_k(&row.form) {
            rows.push(row);
        }
    }
    rows
}

fn parse_index_line(line: &str) -> Option<IndexRow> {
    let mut parts = line.splitn(5, '|');
    let cik = pad_cik(parts.next()?.trim());
    let company_name = parts.next()?.trim().to_string();
    let form = parts.next()?.trim().to_string();
    let filed_raw = parts.next()?.trim();
    let filename = parts.next()?.trim().to_string();
    if cik.chars().all(|c| c == '0') || filename.is_empty() {
        return None;
    }
    let filed_date = normalize_filed_date(filed_raw)?;
    Some(IndexRow {
        cik,
        company_name,
        form,
        filed_date,
        filename,
    })
}

fn normalize_filed_date(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 8 {
        return Some(format!(
            "{}-{}-{}",
            &digits[0..4],
            &digits[4..6],
            &digits[6..8]
        ));
    }
    if raw.len() == 10 && raw.as_bytes().get(4) == Some(&b'-') {
        return Some(raw.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_index_url_uses_quarter() {
        let d = NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        assert_eq!(
            master_index_url(d),
            "https://www.sec.gov/Archives/edgar/daily-index/2026/QTR3/master.20260911.idx"
        );
        let d = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
        assert!(master_index_url(d).contains("QTR1"));
    }

    #[test]
    fn parses_fixture_and_keeps_only_eight_k() {
        let body = include_str!("../fixtures/master.idx");
        let rows = parse_master_index(body);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cik, "0000320193");
        assert_eq!(rows[0].form, "8-K");
        assert_eq!(rows[0].filed_date, "2026-09-11");
        assert_eq!(rows[1].form, "8-K/A");
        assert!(rows.iter().all(|r| is_eight_k(&r.form)));
    }
}
