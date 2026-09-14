//! 8-K SGML header only. Do not NLP the body.
//!
//! Live complete-submission `.txt` files usually carry `ITEM INFORMATION:` titles
//! (not `<ITEMS>N.NN`). Map those titles with the Form 8-K table. Repeatable
//! `<ITEMS>` still wins when present. Unknown titles are ignored (not guessed).

use std::collections::BTreeSet;

/// Official Form 8-K item titles as EDGAR prints them after `ITEM INFORMATION:`.
/// Lowercased, whitespace-collapsed. Not a body regex.
const ITEM_INFORMATION_TITLES: &[(&str, &str)] = &[
    ("entry into a material definitive agreement", "1.01"),
    ("termination of a material definitive agreement", "1.02"),
    ("bankruptcy or receivership", "1.03"),
    (
        "mine safety - reporting of shutdowns and patterns of violations",
        "1.04",
    ),
    (
        "mine safety – reporting of shutdowns and patterns of violations",
        "1.04",
    ),
    ("mine safety disclosure", "1.04"),
    ("material cybersecurity incidents", "1.05"),
    (
        "completion of acquisition or disposition of assets",
        "2.01",
    ),
    ("results of operations and financial condition", "2.02"),
    (
        "creation of a direct financial obligation or an obligation under an off-balance sheet arrangement of a registrant",
        "2.03",
    ),
    (
        "triggering events that accelerate or increase a direct financial obligation or an obligation under an off-balance sheet arrangement",
        "2.04",
    ),
    ("costs associated with exit or disposal activities", "2.05"),
    ("material impairments", "2.06"),
    (
        "notice of delisting or failure to satisfy a continued listing rule or standard; transfer of listing",
        "3.01",
    ),
    ("unregistered sales of equity securities", "3.02"),
    (
        "material modifications to rights of security holders",
        "3.03",
    ),
    ("changes in registrant's certifying accountant", "4.01"),
    ("changes in registrants certifying accountant", "4.01"),
    (
        "non-reliance on previously issued financial statements or a related audit report or completed interim review",
        "4.02",
    ),
    ("changes in control of registrant", "5.01"),
    (
        "departure of directors or certain officers; election of directors; appointment of certain officers; compensatory arrangements of certain officers",
        "5.02",
    ),
    (
        "amendments to articles of incorporation or bylaws; change in fiscal year",
        "5.03",
    ),
    (
        "temporary suspension of trading under registrant's employee benefit plans",
        "5.04",
    ),
    (
        "temporary suspension of trading under registrants employee benefit plans",
        "5.04",
    ),
    (
        "amendments to the registrant's code of ethics, or waiver of a provision of the code of ethics",
        "5.05",
    ),
    (
        "amendments to the registrants code of ethics, or waiver of a provision of the code of ethics",
        "5.05",
    ),
    ("change in shell company status", "5.06"),
    (
        "submission of matters to a vote of security holders",
        "5.07",
    ),
    (
        "shareholder nominations pursuant to exchange act rule 14a-11",
        "5.08",
    ),
    ("abs informational and computational material", "6.01"),
    ("change of servicer or trustee", "6.02"),
    (
        "change in credit enhancement or other external support",
        "6.03",
    ),
    ("failure to make a required distribution", "6.04"),
    ("securities act updating disclosure", "6.05"),
    ("regulation fd disclosure", "7.01"),
    ("other events", "8.01"),
    ("financial statements and exhibits", "9.01"),
];

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
    parse_items_tags(header, &mut set);
    parse_item_information(header, &mut set);
    set.into_iter().collect()
}

fn parse_items_tags(header: &str, set: &mut BTreeSet<String>) {
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
}

fn parse_item_information(header: &str, set: &mut BTreeSet<String>) {
    for line in header.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_ascii_uppercase();
        let Some(rest) = upper.strip_prefix("ITEM INFORMATION:") else {
            continue;
        };
        let title_orig = &trimmed[trimmed.len() - rest.len()..];
        if let Some(code) = lookup_item_title(title_orig) {
            set.insert(code.to_string());
        }
    }
}

fn lookup_item_title(title: &str) -> Option<&'static str> {
    let key = collapse_ws(&title.to_ascii_lowercase());
    if key.is_empty() {
        return None;
    }
    ITEM_INFORMATION_TITLES
        .iter()
        .find(|(t, _)| *t == key)
        .map(|(_, code)| *code)
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
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
    let mut out = String::new();
    let mut saw_dot = false;
    for c in t.chars() {
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
    fn parses_item_information_titles_from_header() {
        let body = "\
<SEC-HEADER>
ITEM INFORMATION:\t\tMaterial Modifications to Rights of Security Holders
ITEM INFORMATION:\t\tAmendments to Articles of Incorporation or Bylaws; Change in Fiscal Year
ITEM INFORMATION:\t\tSubmission of Matters to a Vote of Security Holders
</SEC-HEADER>
<DOCUMENT>
Item 99.99 must not be parsed from the body
</DOCUMENT>
";
        assert_eq!(
            parse_items(body),
            vec!["3.03".to_string(), "5.03".to_string(), "5.07".to_string()]
        );
    }

    #[test]
    fn unknown_item_information_is_not_guessed() {
        let items =
            parse_items("<SEC-HEADER>\nITEM INFORMATION:\t\tNot A Real Item Title\n</SEC-HEADER>");
        assert!(items.is_empty());
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
