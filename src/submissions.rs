//! `data.sec.gov/submissions/CIK*.json` fallback for item lists.

use anyhow::{Context, Result};
use serde_json::Value;

use crate::sgml::normalize_accession;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessionItems {
    /// Accession is not in `filings.recent`.
    Missing,
    /// Accession is present; `None` means an empty item list (store NULL).
    Present(Option<String>),
}

pub fn items_for_accession(body: &str, accession: &str) -> Result<AccessionItems> {
    let v: Value = serde_json::from_str(body).context("submissions JSON")?;
    let want = normalize_accession(accession);
    let recent = v
        .pointer("/filings/recent")
        .ok_or_else(|| anyhow::anyhow!("submissions JSON missing filings.recent"))?;
    let accs = recent
        .get("accessionNumber")
        .and_then(|x| x.as_array())
        .ok_or_else(|| anyhow::anyhow!("submissions JSON missing accessionNumber"))?;
    let items_arr = recent.get("items").and_then(|x| x.as_array());
    for (i, a) in accs.iter().enumerate() {
        let got = a.as_str().unwrap_or("");
        if normalize_accession(got) != want {
            continue;
        }
        let raw = items_arr
            .and_then(|arr| arr.get(i))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        return Ok(AccessionItems::Present(normalize_items_csv(raw)));
    }
    Ok(AccessionItems::Missing)
}

fn normalize_items_csv(raw: &str) -> Option<String> {
    let mut set = std::collections::BTreeSet::new();
    for part in raw.split(',') {
        let t = part.trim();
        if t.is_empty() {
            continue;
        }
        let digits: String = t
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if digits.contains('.') {
            set.insert(digits);
        }
    }
    if set.is_empty() {
        None
    } else {
        Some(set.into_iter().collect::<Vec<_>>().join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_items_for_matching_accession() {
        let body = include_str!("../fixtures/submissions-aapl.json");
        match items_for_accession(body, "0000320193-26-000123").unwrap() {
            AccessionItems::Present(Some(items)) => assert_eq!(items, "2.01,9.01"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            items_for_accession(body, "0000320193-26-999999").unwrap(),
            AccessionItems::Missing
        );
    }
}
