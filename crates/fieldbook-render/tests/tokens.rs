//! Design-token parity, contrast and token-use tests (`build-tokens`).

use std::collections::{BTreeMap, BTreeSet};

const APP_CSS: &str = include_str!("../assets/app.css");
const HANDOFF_MD: &str = include_str!("../../../docs/design/HANDOFF.md");
const MAP_MD: &str = include_str!("../../../docs/design/MAP.md");

#[derive(Debug, Clone, PartialEq, Eq)]
struct CssBlock {
    media: Option<String>,
    selector: String,
    declarations: Vec<(String, String)>,
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn find_matching_brace(s: &str, open_idx: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut in_quote: Option<u8> = None;
    let mut i = open_idx;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q {
                in_quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            in_quote = Some(b);
        } else if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn parse_declarations(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut paren_depth = 0usize;
    for ch in body.chars() {
        if let Some(q) = in_quote {
            current.push(ch);
            if ch == q {
                in_quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_quote = Some(ch);
                current.push(ch);
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            ';' if paren_depth == 0 => {
                let stmt = current.trim();
                if let Some((prop, val)) = stmt.split_once(':') {
                    out.push((prop.trim().to_owned(), val.trim().to_owned()));
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let tail = current.trim();
    if let Some((prop, val)) = tail.split_once(':') {
        out.push((prop.trim().to_owned(), val.trim().to_owned()));
    }
    out
}

fn parse_css_blocks(css: &str) -> Vec<CssBlock> {
    let clean = strip_comments(css);
    let mut blocks = Vec::new();
    let mut pos = 0usize;
    while pos < clean.len() {
        let rest = &clean[pos..];
        let Some(rel_open) = rest.find('{') else {
            break;
        };
        let open_idx = pos + rel_open;
        let header = clean[pos..open_idx].trim();
        let Some(close_idx) = find_matching_brace(&clean, open_idx) else {
            panic!("unbalanced braces in CSS");
        };
        let inner = &clean[open_idx + 1..close_idx];
        if header.starts_with("@media") || header.starts_with("@container") {
            let mut inner_pos = 0usize;
            while inner_pos < inner.len() {
                let irest = &inner[inner_pos..];
                let Some(irel_open) = irest.find('{') else {
                    break;
                };
                let iopen = inner_pos + irel_open;
                let selector = inner[inner_pos..iopen].trim().to_owned();
                let Some(iclose) = find_matching_brace(inner, iopen) else {
                    panic!("unbalanced inner braces in CSS");
                };
                let decl_body = &inner[iopen + 1..iclose];
                blocks.push(CssBlock {
                    media: Some(header.to_owned()),
                    selector,
                    declarations: parse_declarations(decl_body),
                });
                inner_pos = iclose + 1;
            }
        } else if header.starts_with("@keyframes") {
            // Skip keyframes rules in block parser.
        } else {
            blocks.push(CssBlock {
                media: None,
                selector: header.to_owned(),
                declarations: parse_declarations(inner),
            });
        }
        pos = close_idx + 1;
    }
    blocks
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_token_block(block: &CssBlock) -> bool {
    let media = block.media.as_deref().map(normalize_ws);
    let sel = normalize_ws(&block.selector);
    match media.as_deref() {
        Some("@media screen and (prefers-color-scheme: dark)") => {
            sel == ":root:not([data-theme=\"light\"])"
        }
        Some("@media screen") => sel == ":root[data-theme=\"dark\"]",
        Some("@media (forced-colors: active)") => {
            sel == ":root, :root:not([data-theme=\"light\"]), :root[data-theme=\"dark\"]"
                || sel == ":root,:root:not([data-theme=\"light\"]),:root[data-theme=\"dark\"]"
        }
        None
        | Some("@media print")
        | Some("@media (prefers-reduced-motion: reduce)")
        | Some("@media (max-width: 767px)") => sel == ":root",
        _ => false,
    }
}

fn token_map(block: &CssBlock) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (k, v) in &block.declarations {
        if k.starts_with("--") {
            map.insert(k.clone(), v.clone());
        }
    }
    map
}

fn strip_ticks(cell: &str) -> String {
    let trimmed = cell.trim();
    if let Some(inner) = trimmed.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
        inner.to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[derive(Debug)]
struct HandoffTables {
    light: BTreeMap<String, String>,
    dark: BTreeMap<String, String>,
    print: BTreeMap<String, String>,
    forced_colors: BTreeMap<String, String>,
    reduced_motion: BTreeMap<String, String>,
}

fn extract_section_4(md: &str) -> &str {
    let Some(start) = md.find("\n## 4. Design tokens\n") else {
        panic!("HANDOFF §4 heading missing");
    };
    let rest = &md[start + 1..];
    let end = rest[1..].find("\n## ").map_or(rest.len(), |idx| idx + 1);
    &rest[..end]
}

fn parse_handoff_tables(md: &str) -> HandoffTables {
    let sec4 = extract_section_4(md);
    let mut tables = HandoffTables {
        light: BTreeMap::new(),
        dark: BTreeMap::new(),
        print: BTreeMap::new(),
        forced_colors: BTreeMap::new(),
        reduced_motion: BTreeMap::new(),
    };

    let mut current_subsection = "";
    let lines: Vec<&str> = sec4.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(h) = line.strip_prefix("### ") {
            current_subsection = h.trim();
            i += 1;
            continue;
        }
        if current_subsection.starts_with("Brand layer") {
            i += 1;
            continue;
        }
        if line.starts_with("| Token |") {
            let cols: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let second_col = cols.get(1).copied().unwrap_or("");
            i += 2; // skip header and separator row
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let row: Vec<&str> = lines[i]
                    .trim()
                    .trim_matches('|')
                    .split('|')
                    .map(str::trim)
                    .collect();
                if row.len() >= 2 {
                    let name = strip_ticks(row[0]);
                    let val1 = strip_ticks(row[1]);
                    match second_col {
                        "Light" => {
                            let val2 = strip_ticks(row[2]);
                            tables.light.insert(name.clone(), val1);
                            tables.dark.insert(name, val2);
                        }
                        "Value" => {
                            tables.light.insert(name, val1);
                        }
                        "Print" => {
                            tables.print.insert(name, val1);
                        }
                        "Forced colours" => {
                            tables.forced_colors.insert(name, val1);
                        }
                        "Reduced motion" => {
                            tables.reduced_motion.insert(name, val1);
                        }
                        other => panic!("unexpected HANDOFF §4 token table column: {other}"),
                    }
                }
                i += 1;
            }
            continue;
        }
        i += 1;
    }

    tables
}

fn check_phone_drawer_override(css: &str) -> Result<(), String> {
    let blocks = parse_css_blocks(css);
    let root_block = blocks
        .iter()
        .find(|b| b.media.is_none() && normalize_ws(&b.selector) == ":root")
        .ok_or_else(|| "missing base :root block".to_owned())?;
    let root_tokens = token_map(root_block);
    let Some(base_drawer) = root_tokens.get("--drawer-w") else {
        return Err("missing base --drawer-w on :root".to_owned());
    };
    if base_drawer != "440px" {
        return Err(format!(
            "expected base --drawer-w: 440px, got {base_drawer}"
        ));
    }

    let mut phone_root_blocks = Vec::new();
    for b in &blocks {
        if normalize_ws(&b.selector) == ":root"
            && let Some(m) = &b.media
        {
            let norm = normalize_ws(m);
            if norm.contains("max-width") {
                phone_root_blocks.push((norm, b));
            }
        }
    }

    if phone_root_blocks.is_empty() {
        return Err("missing phone :root override block".to_owned());
    }
    if phone_root_blocks.len() > 1 {
        return Err("multiple max-width :root blocks found".to_owned());
    }

    let (media, block) = &phone_root_blocks[0];
    if media != "@media (max-width: 767px)" {
        return Err(format!(
            "wrong phone breakpoint: expected @media (max-width: 767px), got {media}"
        ));
    }

    if block.declarations.len() != 1 {
        return Err(format!(
            "phone :root block must contain only --drawer-w, found {} declarations",
            block.declarations.len()
        ));
    }
    let (prop, val) = &block.declarations[0];
    if prop != "--drawer-w" {
        return Err(format!("unrelated property in phone :root block: {prop}"));
    }
    if val != "100vw" {
        return Err(format!(
            "wrong phone --drawer-w value: expected 100vw, got {val}"
        ));
    }

    Ok(())
}

fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
    let Some(h) = hex.strip_prefix('#') else {
        panic!("hex color must start with #: {hex}");
    };
    assert_eq!(h.len(), 6, "expected 6-digit hex color, got {hex}");
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or_else(|e| panic!("bad R in {hex}: {e}"));
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or_else(|e| panic!("bad G in {hex}: {e}"));
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or_else(|e| panic!("bad B in {hex}: {e}"));
    (r, g, b)
}

fn channel_luminance(c: u8) -> f64 {
    let s = f64::from(c) / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn relative_luminance(rgb: (u8, u8, u8)) -> f64 {
    0.2126 * channel_luminance(rgb.0)
        + 0.7152 * channel_luminance(rgb.1)
        + 0.0722 * channel_luminance(rgb.2)
}

fn contrast_ratio(hex_a: &str, hex_b: &str) -> f64 {
    let la = relative_luminance(parse_hex_color(hex_a));
    let lb = relative_luminance(parse_hex_color(hex_b));
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

fn is_token_only_property(prop: &str) -> bool {
    matches!(
        prop,
        "font"
            | "font-family"
            | "font-size"
            | "font-weight"
            | "line-height"
            | "letter-spacing"
            | "padding"
            | "padding-top"
            | "padding-right"
            | "padding-bottom"
            | "padding-left"
            | "padding-inline"
            | "padding-inline-start"
            | "padding-inline-end"
            | "padding-block"
            | "padding-block-start"
            | "padding-block-end"
            | "margin"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "margin-inline"
            | "margin-inline-start"
            | "margin-inline-end"
            | "margin-block"
            | "margin-block-start"
            | "margin-block-end"
            | "gap"
            | "row-gap"
            | "column-gap"
            | "border"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "border-width"
            | "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width"
            | "outline"
            | "outline-width"
            | "outline-offset"
            | "border-radius"
            | "border-top-left-radius"
            | "border-top-right-radius"
            | "border-bottom-left-radius"
            | "border-bottom-right-radius"
            | "box-shadow"
            | "z-index"
            | "transition"
            | "transition-duration"
            | "animation"
            | "animation-duration"
    )
}

fn strip_var_and_calc(value: &str) -> String {
    let mut s = value.to_owned();
    // Strip var(--...) calls.
    while let Some(idx) = s.find("var(--") {
        let rest = &s[idx..];
        if let Some(close) = rest.find(')') {
            s.replace_range(idx..idx + close + 1, "TOKEN");
        } else {
            break;
        }
    }
    s
}

fn value_has_forbidden_raw_number(prop: &str, value: &str) -> bool {
    let trimmed = value.trim();
    if matches!(trimmed, "0" | "auto" | "inherit" | "none" | "normal") {
        return false;
    }

    // Check if inside calc(...) only tokens and arithmetic operators (+, -, *, /) or -1 / 2 multipliers are used,
    // or if there is any raw unit literal like 7px, 12px, 1.5, 400, etc. outside allowed tokens.
    let stripped = strip_var_and_calc(trimmed);
    for raw_part in stripped
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '/' | '(' | ')' | '+' | '*'))
    {
        let part = raw_part.trim_matches('-');
        if part.is_empty() || part == "TOKEN" || part == "calc" || part == "inset" {
            continue;
        }
        if matches!(
            part,
            "0" | "auto"
                | "inherit"
                | "none"
                | "normal"
                | "solid"
                | "dashed"
                | "dotted"
                | "transparent"
                | "currentColor"
                | "infinite"
                | "linear"
                | "opacity"
                | "transform"
                | "left"
                | "margin-right"
        ) {
            continue;
        }
        if let Some(num) = part.strip_suffix('%')
            && num.parse::<f64>().is_ok()
        {
            continue;
        }
        // Any token containing ASCII digits (e.g. `7px`, `15px`, `400`, `1.5`, `120ms`, `10`) is a raw number.
        if part.chars().any(|c| c.is_ascii_digit()) {
            let _ = prop;
            return true;
        }
    }
    false
}

fn contains_colour_literal(val: &str) -> bool {
    let bytes = val.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'#' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() {
            return true;
        }
    }
    let lower = val.to_ascii_lowercase();
    lower.contains("rgb(")
        || lower.contains("rgba(")
        || lower.contains("hsl(")
        || lower.contains("hsla(")
}

mod tokens {
    use super::*;

    #[test]
    fn values_match_handoff() {
        let handoff = parse_handoff_tables(HANDOFF_MD);
        let blocks = parse_css_blocks(APP_CSS);

        let Some(root_block) = blocks
            .iter()
            .find(|b| b.media.is_none() && normalize_ws(&b.selector) == ":root")
        else {
            panic!(":root block in app.css");
        };
        let root_map = token_map(root_block);

        for (name, expected) in &handoff.light {
            let actual = root_map
                .get(name)
                .unwrap_or_else(|| panic!("token {name} missing from :root in app.css"));
            assert_eq!(
                actual, expected,
                "token {name} in :root has value {actual:?}, expected {expected:?} from HANDOFF.md"
            );
        }
        for name in root_map.keys() {
            assert!(
                handoff.light.contains_key(name),
                "extra token {name} in :root not in HANDOFF.md"
            );
        }

        let dark_prefers = blocks
            .iter()
            .find(|b| {
                b.media.as_deref().map(normalize_ws).as_deref()
                    == Some("@media screen and (prefers-color-scheme: dark)")
                    && normalize_ws(&b.selector) == ":root:not([data-theme=\"light\"])"
            })
            .expect("prefers-color-scheme dark block in app.css");
        let dark_map = token_map(dark_prefers);
        for (name, expected) in &handoff.dark {
            let actual = dark_map
                .get(name)
                .unwrap_or_else(|| panic!("token {name} missing from dark block in app.css"));
            assert_eq!(
                actual, expected,
                "token {name} in dark block has value {actual:?}, expected {expected:?} from HANDOFF.md"
            );
        }
        for name in dark_map.keys() {
            assert!(
                handoff.dark.contains_key(name),
                "extra token {name} in dark block not in HANDOFF.md"
            );
        }
        assert!(
            dark_prefers
                .declarations
                .iter()
                .any(|(k, v)| k == "color-scheme" && v == "dark"),
            "dark prefers-color-scheme block must set color-scheme: dark"
        );

        let print_block = blocks
            .iter()
            .find(|b| {
                b.media.as_deref().map(normalize_ws).as_deref() == Some("@media print")
                    && normalize_ws(&b.selector) == ":root"
            })
            .expect("@media print :root block in app.css");
        let print_map = token_map(print_block);
        for (name, expected) in &handoff.print {
            let actual = print_map
                .get(name)
                .unwrap_or_else(|| panic!("token {name} missing from @media print in app.css"));
            assert_eq!(
                actual, expected,
                "token {name} in @media print has value {actual:?}, expected {expected:?}"
            );
        }
        for name in print_map.keys() {
            assert!(
                handoff.print.contains_key(name),
                "extra token {name} in @media print not in HANDOFF.md"
            );
        }

        let forced_block = blocks
            .iter()
            .find(|b| {
                b.media.as_deref().map(normalize_ws).as_deref()
                    == Some("@media (forced-colors: active)")
            })
            .expect("@media (forced-colors: active) block in app.css");
        let forced_map = token_map(forced_block);
        for (name, expected) in &handoff.forced_colors {
            let actual = forced_map.get(name).unwrap_or_else(|| {
                panic!("token {name} missing from @media (forced-colors: active) in app.css")
            });
            assert_eq!(
                actual, expected,
                "token {name} in @media (forced-colors: active) has value {actual:?}, expected {expected:?}"
            );
        }
        for name in forced_map.keys() {
            assert!(
                handoff.forced_colors.contains_key(name),
                "extra token {name} in @media (forced-colors: active) not in HANDOFF.md"
            );
        }

        let reduced_block = blocks
            .iter()
            .find(|b| {
                b.media.as_deref().map(normalize_ws).as_deref()
                    == Some("@media (prefers-reduced-motion: reduce)")
                    && normalize_ws(&b.selector) == ":root"
            })
            .expect("@media (prefers-reduced-motion: reduce) :root block in app.css");
        let reduced_map = token_map(reduced_block);
        for (name, expected) in &handoff.reduced_motion {
            let actual = reduced_map.get(name).unwrap_or_else(|| {
                panic!(
                    "token {name} missing from @media (prefers-reduced-motion: reduce) in app.css"
                )
            });
            assert_eq!(
                actual, expected,
                "token {name} in @media (prefers-reduced-motion: reduce) has value {actual:?}, expected {expected:?}"
            );
        }
        for name in reduced_map.keys() {
            assert!(
                handoff.reduced_motion.contains_key(name),
                "extra token {name} in @media (prefers-reduced-motion: reduce) not in HANDOFF.md"
            );
        }

        // If MAP.md Design tokens section holds a token table, it must agree too.
        if let Some(map_sec_idx) = MAP_MD.find("## Design tokens") {
            let map_sec = &MAP_MD[map_sec_idx..];
            for line in map_sec.lines() {
                if line.trim().starts_with("| `--") {
                    let cols: Vec<&str> = line
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect();
                    if cols.len() >= 2 {
                        let name = strip_ticks(cols[0]);
                        let val = strip_ticks(cols[1]);
                        assert_eq!(
                            root_map.get(&name),
                            Some(&val),
                            "MAP.md token {name} disagrees with app.css"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn each_token_defined_once_per_block() {
        for block in parse_css_blocks(APP_CSS) {
            let mut seen = BTreeSet::new();
            for (prop, _) in &block.declarations {
                if prop.starts_with("--") {
                    assert!(
                        seen.insert(prop.clone()),
                        "duplicate token {prop} in block {:?} {}",
                        block.media,
                        block.selector
                    );
                }
            }
        }
    }

    #[test]
    fn every_var_is_defined() {
        let blocks = parse_css_blocks(APP_CSS);
        let root_block = blocks
            .iter()
            .find(|b| b.media.is_none() && normalize_ws(&b.selector) == ":root")
            .expect(":root block");
        let defined: BTreeSet<String> = token_map(root_block).into_keys().collect();

        let clean = strip_comments(APP_CSS);
        let mut rest = clean.as_str();
        while let Some(idx) = rest.find("var(") {
            let after = &rest[idx + 4..];
            let Some(end) = after.find([',', ')']) else {
                panic!("missing closing ) for var() in app.css");
            };
            let name = after[..end].trim();
            assert!(
                defined.contains(name),
                "var({name}) in app.css is not defined on :root"
            );
            rest = &after[end..];
        }
    }

    #[test]
    fn dark_blocks_are_equal() {
        let blocks = parse_css_blocks(APP_CSS);
        let prefers = blocks
            .iter()
            .find(|b| {
                b.media.as_deref().map(normalize_ws).as_deref()
                    == Some("@media screen and (prefers-color-scheme: dark)")
                    && normalize_ws(&b.selector) == ":root:not([data-theme=\"light\"])"
            })
            .expect("prefers-color-scheme dark block");
        let explicit = blocks
            .iter()
            .find(|b| {
                b.media.as_deref().map(normalize_ws).as_deref() == Some("@media screen")
                    && normalize_ws(&b.selector) == ":root[data-theme=\"dark\"]"
            })
            .expect("explicit data-theme dark block");
        assert_eq!(
            prefers.declarations, explicit.declarations,
            "the two dark blocks in app.css must have identical declarations"
        );
    }

    #[test]
    fn no_custom_property_outside_the_blocks() {
        for block in parse_css_blocks(APP_CSS) {
            if is_token_block(&block) {
                continue;
            }
            for (prop, _) in &block.declarations {
                assert!(
                    !prop.starts_with("--"),
                    "custom property {prop} defined outside token blocks in rule {:?} {}",
                    block.media,
                    block.selector
                );
            }
        }
    }

    #[test]
    fn no_colour_literal_outside_the_blocks() {
        for block in parse_css_blocks(APP_CSS) {
            if is_token_block(&block) {
                continue;
            }
            for (prop, val) in &block.declarations {
                assert!(
                    !contains_colour_literal(val),
                    "colour literal in rule {:?} {} property {prop}: {val}",
                    block.media,
                    block.selector
                );
            }
        }
    }

    #[test]
    fn text_pairs_reach_4_5() {
        let blocks = parse_css_blocks(APP_CSS);
        let root_map = token_map(
            blocks
                .iter()
                .find(|b| b.media.is_none() && normalize_ws(&b.selector) == ":root")
                .expect(":root block"),
        );
        let dark_map = token_map(
            blocks
                .iter()
                .find(|b| {
                    b.media.as_deref().map(normalize_ws).as_deref() == Some("@media screen")
                        && normalize_ws(&b.selector) == ":root[data-theme=\"dark\"]"
                })
                .expect("dark block"),
        );

        let text_tokens = ["fg", "muted", "accent", "ok", "warn", "bad", "info"];
        let bg_tokens = ["bg", "surface", "surface-2"];
        let status_tokens = ["ok", "warn", "bad", "info"];

        let mut failures = Vec::new();
        for (theme, map) in [("light", &root_map), ("dark", &dark_map)] {
            for fg in &text_tokens {
                let fg_val = &map[&format!("--color-{fg}")];
                for bg in &bg_tokens {
                    let bg_val = &map[&format!("--color-{bg}")];
                    let ratio = contrast_ratio(fg_val, bg_val);
                    if ratio < 4.5 {
                        failures.push(format!(
                            "text pair {fg} on {bg} in {theme} has contrast {ratio:.2}:1 (< 4.5:1)"
                        ));
                    }
                }
            }
            for st in &status_tokens {
                let fg_val = &map[&format!("--color-{st}")];
                let bg_val = &map[&format!("--color-{st}-soft")];
                let ratio = contrast_ratio(fg_val, bg_val);
                if ratio < 4.5 {
                    failures.push(format!(
                        "status pair {st} on {st}-soft in {theme} has contrast {ratio:.2}:1 (< 4.5:1)"
                    ));
                }
            }
            for acc in ["accent", "accent-hover"] {
                let fg_val = &map["--color-accent-fg"];
                let bg_val = &map[&format!("--color-{acc}")];
                let ratio = contrast_ratio(fg_val, bg_val);
                if ratio < 4.5 {
                    failures.push(format!(
                        "pair accent-fg on {acc} in {theme} has contrast {ratio:.2}:1 (< 4.5:1)"
                    ));
                }
            }
            let fg_val = &map["--color-fg"];
            let mark_val = &map["--color-mark"];
            let ratio = contrast_ratio(fg_val, mark_val);
            if ratio < 4.5 {
                failures.push(format!(
                    "pair fg on mark in {theme} has contrast {ratio:.2}:1 (< 4.5:1)"
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "contrast failures:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn token_use_lint() {
        for block in parse_css_blocks(APP_CSS) {
            if is_token_block(&block) {
                continue;
            }
            for (prop, val) in &block.declarations {
                if is_token_only_property(prop) {
                    assert!(
                        !value_has_forbidden_raw_number(prop, val),
                        "raw number in rule {:?} {} property {prop}: {val}",
                        block.media,
                        block.selector
                    );
                }
            }
        }
    }

    #[test]
    fn ui_pairs_reach_3() {
        let blocks = parse_css_blocks(APP_CSS);
        let root_map = token_map(
            blocks
                .iter()
                .find(|b| b.media.is_none() && normalize_ws(&b.selector) == ":root")
                .expect(":root block"),
        );
        let dark_map = token_map(
            blocks
                .iter()
                .find(|b| {
                    b.media.as_deref().map(normalize_ws).as_deref() == Some("@media screen")
                        && normalize_ws(&b.selector) == ":root[data-theme=\"dark\"]"
                })
                .expect("dark block"),
        );

        let ui_tokens = [
            ("focus", "--color-focus"),
            ("chart-1", "--chart-1"),
            ("chart-neutral", "--chart-neutral"),
            ("muted", "--color-muted"),
        ];
        let bg_tokens = ["bg", "surface", "surface-2"];

        for (theme, map) in [("light", &root_map), ("dark", &dark_map)] {
            for (ui_label, ui_key) in &ui_tokens {
                let ui_val = &map[*ui_key];
                for bg in &bg_tokens {
                    let bg_val = &map[&format!("--color-{bg}")];
                    let ratio = contrast_ratio(ui_val, bg_val);
                    assert!(
                        ratio >= 3.0,
                        "UI pair {ui_label} on {bg} in {theme} has contrast {ratio:.2}:1 (< 3:1)"
                    );
                }
            }
        }
    }

    #[test]
    fn no_web_font() {
        let lower = APP_CSS.to_ascii_lowercase();
        assert!(
            !lower.contains("@font-face"),
            "app.css must not contain @font-face"
        );
        assert!(!lower.contains("url("), "app.css must not contain url(");
    }

    #[test]
    fn phone_drawer_override_matches() {
        check_phone_drawer_override(APP_CSS)
            .unwrap_or_else(|e| panic!("phone drawer override check failed on app.css: {e}"));

        // Fixture 1: missing override
        let missing = ":root { --drawer-w: 440px; }";
        assert!(
            check_phone_drawer_override(missing).is_err(),
            "missing phone override must fail"
        );

        // Fixture 2: wrong breakpoint
        let wrong_bp = ":root { --drawer-w: 440px; } @media (max-width: 720px) { :root { --drawer-w: 100vw; } }";
        assert!(
            check_phone_drawer_override(wrong_bp).is_err(),
            "wrong phone breakpoint must fail"
        );

        // Fixture 3: wrong value
        let wrong_val = ":root { --drawer-w: 440px; } @media (max-width: 767px) { :root { --drawer-w: 90vw; } }";
        assert!(
            check_phone_drawer_override(wrong_val).is_err(),
            "wrong phone --drawer-w value must fail"
        );

        // Fixture 4: unrelated custom property
        let extra_prop = ":root { --drawer-w: 440px; } @media (max-width: 767px) { :root { --drawer-w: 100vw; --sidebar-w: 100vw; } }";
        assert!(
            check_phone_drawer_override(extra_prop).is_err(),
            "unrelated custom property in phone override must fail"
        );
    }
}
