//! Identity shape scan (`FB0401`, `docs/reference/corpus-format.md#scans`).
//!
//! **Responsibility.** Hold the [`SHAPES`] table in the order of the "Identity shapes"
//! table in `docs/reference/corpus-format.md#scans`, together with the tracker-key
//! prefix exemption lists ([`FAMILY_PREFIXES`], [`CONTROL_FAMILY_PREFIXES`],
//! [`STANDARD_PREFIXES`]), and scan hand-written Corpus files for identity shapes.
//!
//! **Rules.** Hand-written matchers only; no regex, no I/O, no clock, no environment.

use super::Finding;

/// One row of the "Identity shapes" table in `docs/reference/corpus-format.md#scans`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityShapeRow {
    /// The `Shape` column of the reference table.
    pub shape: &'static str,
    /// The `Matches` column of the reference table.
    pub matches: &'static str,
    /// The `Does not match` column of the reference table.
    pub does_not_match: &'static str,
}

/// The six rows of the "Identity shapes" table in `docs/reference/corpus-format.md#scans`,
/// in table order.
pub const SHAPES: &[IdentityShapeRow] = &[
    IdentityShapeRow {
        shape: "Email address",
        matches: "`name@host.tld`",
        does_not_match: "A `noreply` or `no-reply` local part, and the host `users.noreply.github.com`",
    },
    IdentityShapeRow {
        shape: "Phone number",
        matches: "A leading `+` and at least nine digits; a parenthesised area code and seven digits; three, three and four digits joined by hyphens",
        does_not_match: "A dotted clause number such as `252.204-7012`",
    },
    IdentityShapeRow {
        shape: "Private IPv4 address",
        matches: "Four dotted decimal fields in `10/8`, `172.16/12`, `192.168/16`, `127/8` or `169.254/16`",
        does_not_match: "A public address",
    },
    IdentityShapeRow {
        shape: "Internal host name",
        matches: "A name that ends `.lan`, `.local`, `.internal` or `.home`",
        does_not_match: "",
    },
    IdentityShapeRow {
        shape: "Home-directory path",
        matches: r"`C:\Users\` or `/home/` or `/Users/` followed by a plain name",
        does_not_match: "A placeholder in angle brackets or percent signs",
    },
    IdentityShapeRow {
        shape: "Tracker key",
        matches: "Two to ten capitals, a hyphen and one to six digits, with no letter, digit or hyphen on either side",
        does_not_match: "Every prefix in the const `FAMILIES` table (`Q`, `XC`, `CL`, `FB`, `CALC`, `QA`, `FC`, the reserved case prefixes `EST`, `QTE`, `SCH`, `RTE`, `EAC`, `EVM` and `CTF`, and the company prefixes); the control families `AC AT AU CA CM CP IA IR MA MP PE PL PM PS PT RA SA SC SI SR`; and `DD`, `SF`, `ISO`, `AS`, `FIPS`, `RFC`, `BIS`, `DS`, `EEO`, `EIA`, `FSPM`, `HSPD`, `LS`, `MAR`, `NQA`, `SHA`, `VETS`, `WH`. No tracker key prefix is exempt",
    },
];

/// Every identifier family prefix from [`crate::ids::FAMILIES`] (including 1-letter
/// prefixes and lowercase prefixes so `tracker_exemption_lists_every_family_prefix`
/// verifies completeness against `FAMILIES`).
pub const FAMILY_PREFIXES: &[&str] = &[
    "S", "CL", "Q", "XC", "CALC", "QA", "FC", "EST", "QTE", "SCH", "RTE", "EAC", "EVM", "CTF",
    "FB", "edition", "v",
];

/// The 20 NIST SP 800-171 / 800-53 control family prefixes exempt from tracker-key matching.
pub const CONTROL_FAMILY_PREFIXES: &[&str] = &[
    "AC", "AT", "AU", "CA", "CM", "CP", "IA", "IR", "MA", "MP", "PE", "PL", "PM", "PS", "PT", "RA",
    "SA", "SC", "SI", "SR",
];

/// Standard form and specification prefixes exempt from tracker-key matching.
pub const STANDARD_PREFIXES: &[&str] = &[
    "DD", "SF", "ISO", "AS", "FIPS", "RFC", "BIS", "DS", "EEO", "EIA", "FSPM", "HSPD", "LS", "MAR",
    "NQA", "SHA", "VETS", "WH",
];

/// Internal host-name suffixes matched by `FB0401`.
pub const INTERNAL_HOST_SUFFIXES: &[&str] = &[".lan", ".local", ".internal", ".home"];

/// Home-directory path prefixes matched by `FB0401`.
pub const HOME_PATH_PREFIXES: &[&str] = &[r"C:\Users\", "/home/", "/Users/"];

/// Hand-written files and folders of the repository Corpus that are read by the
/// identity scan (`FB0401`).
pub const HAND_WRITTEN_EXEMPT_PREFIX: &str = "corpus/sources/";

/// Scan `text` for identity shapes (`FB0401`) and return all matches sorted by byte span.
#[must_use]
pub fn scan(text: &str) -> Vec<Finding> {
    let mut all = Vec::new();
    scan_emails(text, &mut all);
    scan_phones(text, &mut all);
    scan_private_ipv4(text, &mut all);
    scan_internal_hosts(text, &mut all);
    scan_home_paths(text, &mut all);
    scan_tracker_keys(text, &mut all);

    all.sort_by(|a, b| {
        a.span
            .start
            .cmp(&b.span.start)
            .then(b.span.end.cmp(&a.span.end))
    });

    let mut deduped: Vec<Finding> = Vec::with_capacity(all.len());
    for f in all {
        if let Some(last) = deduped.last()
            && f.span.start < last.span.end
        {
            continue;
        }
        deduped.push(f);
    }
    deduped
}

const fn is_email_local_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-')
}

const fn is_host_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'.'
}

fn is_valid_host_with_tld(host: &str) -> bool {
    let Some((_front, tld)) = host.rsplit_once('.') else {
        return false;
    };
    if tld.len() < 2 || !tld.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    for label in host.split('.') {
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return false;
        }
    }
    true
}

fn scan_emails(text: &str, out: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b != b'@' {
            continue;
        }
        let mut start = idx;
        while start > 0 && is_email_local_char(bytes[start - 1]) {
            start -= 1;
        }
        while start < idx && bytes[start] == b'.' {
            start += 1;
        }
        if start == idx || bytes[idx - 1] == b'.' {
            continue;
        }

        let mut end = idx + 1;
        while end < bytes.len() && is_host_char(bytes[end]) {
            end += 1;
        }
        while end > idx + 1 && matches!(bytes[end - 1], b'.' | b'-') {
            end -= 1;
        }
        if end <= idx + 1 {
            continue;
        }

        let local = &text[start..idx];
        let host = &text[idx + 1..end];
        if !is_valid_host_with_tld(host) {
            continue;
        }

        if local.eq_ignore_ascii_case("noreply")
            || local.eq_ignore_ascii_case("no-reply")
            || host.eq_ignore_ascii_case("users.noreply.github.com")
        {
            continue;
        }

        out.push(Finding::new(&text[start..end], start..end));
    }
}

fn scan_phones(text: &str, out: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Shape 1: leading `+` and at least nine digits (with optional single spaces or hyphens between digit groups)
        if bytes[i] == b'+'
            && (i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'+'))
            && i + 1 < bytes.len()
            && bytes[i + 1].is_ascii_digit()
        {
            let mut j = i + 1;
            let mut digits = 0usize;
            let mut last_digit_end = j;
            while j < bytes.len() {
                if bytes[j].is_ascii_digit() {
                    digits += 1;
                    j += 1;
                    last_digit_end = j;
                } else if matches!(bytes[j], b' ' | b'-')
                    && j + 1 < bytes.len()
                    && bytes[j + 1].is_ascii_digit()
                {
                    j += 1;
                } else {
                    break;
                }
            }
            if digits >= 9
                && (last_digit_end == bytes.len() || !bytes[last_digit_end].is_ascii_alphanumeric())
            {
                out.push(Finding::new(&text[i..last_digit_end], i..last_digit_end));
                i = last_digit_end;
                continue;
            }
        }

        // Shape 2: parenthesised area code `(NNN)` and seven digits `NNN-NNNN`, `NNN NNNN`, or `NNNNNNN`
        if bytes[i] == b'('
            && i + 5 <= bytes.len()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
            && bytes[i + 4] == b')'
        {
            let mut j = i + 5;
            if j < bytes.len() && matches!(bytes[j], b' ' | b'-') {
                j += 1;
            }
            if j + 3 <= bytes.len()
                && bytes[j].is_ascii_digit()
                && bytes[j + 1].is_ascii_digit()
                && bytes[j + 2].is_ascii_digit()
            {
                let mut k = j + 3;
                if k < bytes.len() && matches!(bytes[k], b' ' | b'-') {
                    k += 1;
                }
                if k + 4 <= bytes.len()
                    && bytes[k].is_ascii_digit()
                    && bytes[k + 1].is_ascii_digit()
                    && bytes[k + 2].is_ascii_digit()
                    && bytes[k + 3].is_ascii_digit()
                {
                    let end = k + 4;
                    if end == bytes.len()
                        || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'-')
                    {
                        out.push(Finding::new(&text[i..end], i..end));
                        i = end;
                        continue;
                    }
                }
            }
        }

        // Shape 3: three, three and four digits joined by hyphens (`NNN-NNN-NNNN`),
        // excluding dotted clause numbers such as `252.204-7012` (so not preceded by `.`, letter, digit, or `-`).
        if i + 12 <= bytes.len()
            && (i == 0
                || (!bytes[i - 1].is_ascii_alphanumeric()
                    && bytes[i - 1] != b'-'
                    && bytes[i - 1] != b'.'))
            && bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3] == b'-'
            && bytes[i + 4].is_ascii_digit()
            && bytes[i + 5].is_ascii_digit()
            && bytes[i + 6].is_ascii_digit()
            && bytes[i + 7] == b'-'
            && bytes[i + 8].is_ascii_digit()
            && bytes[i + 9].is_ascii_digit()
            && bytes[i + 10].is_ascii_digit()
            && bytes[i + 11].is_ascii_digit()
        {
            let end = i + 12;
            if end == bytes.len()
                || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'-' && bytes[end] != b'.')
            {
                out.push(Finding::new(&text[i..end], i..end));
                i = end;
                continue;
            }
        }

        i += 1;
    }
}

fn parse_octet(bytes: &[u8], pos: usize) -> Option<(u8, usize)> {
    let mut end = pos;
    while end < bytes.len() && end - pos < 3 && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let len = end - pos;
    if len == 0 || (len > 1 && bytes[pos] == b'0') {
        return None;
    }
    let s = std::str::from_utf8(&bytes[pos..end]).ok()?;
    let val = s.parse::<u8>().ok()?;
    Some((val, end))
}

const fn is_private_ipv4(a: u8, b: u8) -> bool {
    a == 10
        || (a == 172 && b >= 16 && b <= 31)
        || (a == 192 && b == 168)
        || a == 127
        || (a == 169 && b == 254)
}

fn scan_private_ipv4(text: &str, out: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit()
            && (i == 0
                || (!bytes[i - 1].is_ascii_alphanumeric()
                    && bytes[i - 1] != b'.'
                    && bytes[i - 1] != b'-'))
            && let Some((o1, p1)) = parse_octet(bytes, i)
            && p1 < bytes.len()
            && bytes[p1] == b'.'
            && let Some((o2, p2)) = parse_octet(bytes, p1 + 1)
            && p2 < bytes.len()
            && bytes[p2] == b'.'
            && let Some((_o3, p3)) = parse_octet(bytes, p2 + 1)
            && p3 < bytes.len()
            && bytes[p3] == b'.'
            && let Some((_o4, end)) = parse_octet(bytes, p3 + 1)
        {
            let right_ok = end == bytes.len()
                || (!bytes[end].is_ascii_alphanumeric()
                    && bytes[end] != b'-'
                    && (bytes[end] != b'.'
                        || end + 1 == bytes.len()
                        || !bytes[end + 1].is_ascii_alphanumeric()));
            if right_ok && is_private_ipv4(o1, o2) {
                out.push(Finding::new(&text[i..end], i..end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

fn is_valid_host_prefix(prefix: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    for label in prefix.split('.') {
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !label.bytes().any(|b| b.is_ascii_alphabetic())
        {
            return false;
        }
    }
    true
}

fn scan_internal_hosts(text: &str, out: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b != b'.' {
            continue;
        }
        for &suffix in INTERNAL_HOST_SUFFIXES {
            let s_bytes = suffix.as_bytes();
            let end = idx + s_bytes.len();
            if end > bytes.len() || !bytes[idx..end].eq_ignore_ascii_case(s_bytes) {
                continue;
            }
            // Check right boundary: not followed by alphanumeric or `-`, and if followed by `.`, not followed by alphanumeric
            if end < bytes.len() {
                let next = bytes[end];
                if next.is_ascii_alphanumeric() || next == b'-' {
                    continue;
                }
                if next == b'.'
                    && end + 1 < bytes.len()
                    && (bytes[end + 1].is_ascii_alphanumeric() || bytes[end + 1] == b'-')
                {
                    continue;
                }
            }

            let mut start = idx;
            while start > 0 && is_host_char(bytes[start - 1]) {
                start -= 1;
            }
            while start < idx && matches!(bytes[start], b'.' | b'-') {
                start += 1;
            }
            if start > 0 && matches!(bytes[start - 1], b'/' | b'\\') {
                // Avoid matching a filesystem path segment unless preceded by `://`
                if !(start >= 3 && &bytes[start - 3..start] == b"://") {
                    continue;
                }
            }
            let prefix = &text[start..idx];
            if is_valid_host_prefix(prefix) {
                out.push(Finding::new(&text[start..end], start..end));
            }
        }
    }
}

const fn is_plain_username_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
}

fn scan_home_paths(text: &str, out: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let mut matched_prefix: Option<&str> = None;
        for &prefix in HOME_PATH_PREFIXES {
            if bytes[i..].starts_with(prefix.as_bytes()) {
                // For `/home/` or `/Users/`, ensure it is not mid-word (e.g., `http://example.com/home/page` is URL path, wait: check left boundary)
                if prefix.starts_with('/')
                    && i > 0
                    && (bytes[i - 1].is_ascii_alphanumeric()
                        || matches!(bytes[i - 1], b'_' | b'-' | b'.' | b'/'))
                {
                    continue;
                }
                matched_prefix = Some(prefix);
                break;
            }
        }
        if let Some(prefix) = matched_prefix {
            let name_start = i + prefix.len();
            if name_start < bytes.len() {
                let first = bytes[name_start];
                // Placeholder in angle brackets `<...>` or percent signs `%...%` does not match.
                if first != b'<' && first != b'%' && first.is_ascii_alphanumeric() {
                    let mut name_end = name_start + 1;
                    while name_end < bytes.len() && is_plain_username_char(bytes[name_end]) {
                        name_end += 1;
                    }
                    // Strip trailing dot if sentence punctuation at end
                    while name_end > name_start && bytes[name_end - 1] == b'.' {
                        name_end -= 1;
                    }
                    if name_end > name_start {
                        out.push(Finding::new(&text[i..name_end], i..name_end));
                        i = name_end;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
}

/// Returns `true` if `prefix` is in [`FAMILY_PREFIXES`], [`CONTROL_FAMILY_PREFIXES`]
/// or [`STANDARD_PREFIXES`].
#[must_use]
pub fn is_exempt_tracker_prefix(prefix: &str) -> bool {
    FAMILY_PREFIXES.contains(&prefix)
        || CONTROL_FAMILY_PREFIXES.contains(&prefix)
        || STANDARD_PREFIXES.contains(&prefix)
}

const fn is_tracker_boundary_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn scan_tracker_keys(text: &str, out: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_uppercase() && (i == 0 || !is_tracker_boundary_char(bytes[i - 1])) {
            let mut p_end = i + 1;
            while p_end < bytes.len() && bytes[p_end].is_ascii_uppercase() {
                p_end += 1;
            }
            let prefix_len = p_end - i;
            if (2..=10).contains(&prefix_len) && p_end < bytes.len() && bytes[p_end] == b'-' {
                let d_start = p_end + 1;
                let mut d_end = d_start;
                while d_end < bytes.len() && bytes[d_end].is_ascii_digit() {
                    d_end += 1;
                }
                let digit_len = d_end - d_start;
                if (1..=6).contains(&digit_len)
                    && (d_end == bytes.len() || !is_tracker_boundary_char(bytes[d_end]))
                {
                    let prefix = &text[i..p_end];
                    if !is_exempt_tracker_prefix(prefix) {
                        out.push(Finding::new(&text[i..d_end], i..d_end));
                    }
                    i = d_end;
                    continue;
                }
            }
        }
        i += 1;
    }
}
