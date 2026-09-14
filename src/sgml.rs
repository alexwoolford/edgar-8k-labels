//! 8-K SGML header: repeatable `<ITEMS>` only. Do not NLP the body.

use std::collections::BTreeSet;

pub fn header_slice(body: &str) -> &str {
    if let Some(end) = body.find("</SEC-HEADER>") {
        return &body[..end];
    }
    if let Some(end) = body.find("<DOCUMENT>") {
        return &body[..end];
    }
    if body.len() > 65_536 {
        &body[..65_536]
    } else {
        body
    }
}

pub fn parse_items(body: &str) -> Vec<String> {
    let header = header_slice(body);
    let mut set = BTreeSet::new();
    let mut rest = header;
    while let Some(i) = rest.to_ascii_uppercase().find("<ITEMS>") {
        let after = &rest[i + 7..];
        let token = after
            .trim_start()
            .split(|c: char| c == '<' || c.is_whitespace())
            .next()
            .unwrap_or("");
        if let Some(item) = normalize_item(token) {
            set.insert(item);
        }
        rest = after;
    }
    set.into_iter().collect()
}

pub fn join_items(items: &[String]) -> Option<String> {
    if items.is_empty() {
        None
    } else {
        Some(items.join(","))
    }
}

fn normalize_item(raw: &str) -> Option<String> {
    let t = raw.trim().trim_end_matches('>').trim();
    let mut chars = t.chars();
    let mut out = String::new();
    let mut saw_dot = false;
    for c in chars.by_ref() {
        if c.is_ascii_digit() {
            out.push(c);
        } else if c == '.' && !saw_dot {
            out.push('.');
            saw_dot = true;
        } else {
            break;
        }
    }
    if !saw_dot {
        return None;
    }
    let (a, b) = out.split_once('.')?;
    if a.is_empty() || b.len() != 2 || !b.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("{a}.{b}"))
}

pub fn normalize_accession(s: &str) -> String {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() == 18 {
        format!("{}-{}-{}", &d[0..10], &d[10..12], &d[12..18])
    } else {
        s.split_whitespace().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repeatable_items_from_fixture() {
        let body = include_str!("../fixtures/aapl-8k.txt");
        let items = parse_items(body);
        assert_eq!(items, vec!["2.01".to_string(), "9.01".to_string()]);
        assert_eq!(join_items(&items).as_deref(), Some("2.01,9.01"));
    }

    #[test]
    fn empty_header_is_none_not_guessed() {
        let items =
            parse_items("<SEC-HEADER>\nACCESSION NUMBER: 0000000000-26-000001\n</SEC-HEADER>");
        assert!(items.is_empty());
        assert!(join_items(&items).is_none());
    }

    #[test]
    fn normalize_accession_18_digits() {
        assert_eq!(
            normalize_accession("0000320193-26-000123"),
            "0000320193-26-000123"
        );
        assert_eq!(
            normalize_accession("000032019326000123"),
            "0000320193-26-000123"
        );
    }
}
