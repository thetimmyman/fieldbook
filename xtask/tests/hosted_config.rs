//! Hosted deploy configuration integration tests (`hosted-deploy`).

#![expect(
    clippy::disallowed_methods,
    reason = "Hosted deploy integration tests read config files"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

const EXPECTED_HEADERS: [(&str, &str); 9] = [
    (
        "Content-Security-Policy",
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'",
    ),
    ("Strict-Transport-Security", "max-age=31536000"),
    ("X-Robots-Tag", "noindex, nofollow"),
    ("Cache-Control", "private, no-cache"),
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
    ("X-Frame-Options", "DENY"),
    (
        "Permissions-Policy",
        "camera=(), microphone=(), geolocation=(), payment=(), usb=(), serial=(), accelerometer=(), gyroscope=(), magnetometer=(), midi=(), display-capture=()",
    ),
    ("Cross-Origin-Opener-Policy", "same-origin"),
];

// control: HO-02
#[test]
fn headers_csp_matches_the_pages_policy_plus_frame_ancestors() {
    let headers_path = repo_root().join("deploy/_headers");
    let headers_content = std::fs::read_to_string(&headers_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", headers_path.display()));

    let csp_line = headers_content
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Content-Security-Policy:"))
        .expect("Content-Security-Policy header in deploy/_headers");

    let header_csp_val = csp_line
        .strip_prefix("Content-Security-Policy:")
        .expect("strip prefix")
        .trim();

    let csp_rs_path = repo_root().join("crates/fieldbook-render/src/csp.rs");
    let csp_rs_content = std::fs::read_to_string(&csp_rs_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", csp_rs_path.display()));

    let marker = "pub const CSP_STATIC: &str = \"";
    let start_idx = csp_rs_content
        .find(marker)
        .expect("CSP_STATIC definition in csp.rs")
        + marker.len();
    let rest = &csp_rs_content[start_idx..];
    let end_idx = rest.find("\";").expect("end of CSP_STATIC literal");
    let csp_static = &rest[..end_idx];

    let expected_csp = format!("{csp_static}; frame-ancestors 'none'");
    assert_eq!(
        header_csp_val, expected_csp,
        "deploy/_headers CSP must match CSP_STATIC + frame-ancestors 'none'"
    );
}

fn check_headers(content: &str) -> Result<(), String> {
    let mut found_headers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "/*" {
            continue;
        }
        let Some((name, val)) = trimmed.split_once(':') else {
            return Err(format!("line without colon: `{trimmed}`"));
        };
        found_headers
            .entry(name.trim().to_string())
            .or_default()
            .push(val.trim().to_string());
    }

    for (name, expected_val) in EXPECTED_HEADERS {
        let Some(values) = found_headers.get(name) else {
            return Err(format!("missing required header `{name}`"));
        };
        if values.len() > 1 {
            return Err(format!(
                "header `{name}` appears more than once ({} times)",
                values.len()
            ));
        }
        if values[0] != expected_val {
            return Err(format!(
                "header `{name}` value mismatch: expected `{expected_val}`, found `{}`",
                values[0]
            ));
        }
    }

    if found_headers.len() != EXPECTED_HEADERS.len() {
        return Err(format!(
            "expected {} distinct headers, found {}: {:?}",
            EXPECTED_HEADERS.len(),
            found_headers.len(),
            found_headers.keys().collect::<Vec<_>>()
        ));
    }

    Ok(())
}

// control: HO-02
#[test]
fn headers_carry_every_hardening_header() {
    let headers_path = repo_root().join("deploy/_headers");
    let content = std::fs::read_to_string(&headers_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", headers_path.display()));

    if let Err(err) = check_headers(&content) {
        panic!("deploy/_headers failed validation: {err}");
    }
}

// control: HO-02
#[test]
fn a_removed_header_is_refused() {
    let headers_path = repo_root().join("deploy/_headers");
    let content = std::fs::read_to_string(&headers_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", headers_path.display()));

    // Specifically verify removal of X-Frame-Options as named in specification.
    let without_xfo: String = content
        .lines()
        .filter(|line| !line.trim().starts_with("X-Frame-Options:"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = match check_headers(&without_xfo) {
        Ok(()) => panic!("expected Err when removing X-Frame-Options"),
        Err(e) => e,
    };
    assert!(
        err.contains("X-Frame-Options"),
        "expected err to contain `X-Frame-Options`, got: {err}"
    );

    // Loop over every one of the 9 header names: removed, doubled, and changed value.
    for (name, _) in EXPECTED_HEADERS {
        // 1. Remove its line
        let removed: String = content
            .lines()
            .filter(|line| !line.trim().starts_with(&format!("{name}:")))
            .collect::<Vec<_>>()
            .join("\n");
        let err = match check_headers(&removed) {
            Ok(()) => panic!("expected Err when removing {name}"),
            Err(e) => e,
        };
        assert!(
            err.contains(name),
            "expected err to contain `{name}` when removed, got: {err}"
        );

        // 2. Doubled line
        let mut doubled_lines = Vec::new();
        for line in content.lines() {
            doubled_lines.push(line);
            if line.trim().starts_with(&format!("{name}:")) {
                doubled_lines.push(line);
            }
        }
        let doubled = doubled_lines.join("\n");
        let err = match check_headers(&doubled) {
            Ok(()) => panic!("expected Err when doubling {name}"),
            Err(e) => e,
        };
        assert!(
            err.contains(name),
            "expected err to contain `{name}` when doubled, got: {err}"
        );

        // 3. Changed value
        let changed: String = content
            .lines()
            .map(|line| {
                if line.trim().starts_with(&format!("{name}:")) {
                    format!("{name}: altered-value")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let err = match check_headers(&changed) {
            Ok(()) => panic!("expected Err when changing value of {name}"),
            Err(e) => e,
        };
        assert!(
            err.contains(name),
            "expected err to contain `{name}` when value changed, got: {err}"
        );
    }
}

fn check_wrangler(content: &str) -> Result<(), String> {
    let stripped: String = content
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    if !stripped.contains("\"directory\": \"../out/hosted\"") {
        return Err(
            "wrangler configuration must contain `\"directory\": \"../out/hosted\"`".to_string(),
        );
    }

    if stripped.contains("\"workers_dev\": true") {
        return Err("wrangler configuration must not contain `\"workers_dev\": true`".to_string());
    }
    if !stripped.contains("\"workers_dev\": false") {
        return Err("wrangler configuration must contain `\"workers_dev\": false`".to_string());
    }

    if stripped.contains("\"preview_urls\": true") {
        return Err("wrangler configuration must not contain `\"preview_urls\": true`".to_string());
    }
    if !stripped.contains("\"preview_urls\": false") {
        return Err("wrangler configuration must contain `\"preview_urls\": false`".to_string());
    }

    for forbidden in ["\"routes\"", "\"route\"", "\"account_id\"", "\"main\""] {
        if stripped.contains(forbidden) {
            return Err(format!(
                "wrangler configuration must not contain `{forbidden}`"
            ));
        }
    }

    Ok(())
}

// control: HO-08
#[test]
fn wrangler_config_disables_workers_dev_and_previews_and_names_no_account() {
    let wrangler_path = repo_root().join("deploy/wrangler.jsonc");
    let content = std::fs::read_to_string(&wrangler_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", wrangler_path.display()));

    let res = check_wrangler(&content);
    assert!(
        res.is_ok(),
        "deploy/wrangler.jsonc failed validation: {}",
        res.as_ref().err().map_or("", String::as_str)
    );
}

// control: HO-08
#[test]
fn a_planted_wrangler_violation_is_refused() {
    let wrangler_path = repo_root().join("deploy/wrangler.jsonc");
    let content = std::fs::read_to_string(&wrangler_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", wrangler_path.display()));

    let assert_plant = |planted: &str, expected_key: &str| {
        assert_ne!(
            planted, content,
            "planted content for `{expected_key}` must differ from original"
        );
        let err = match check_wrangler(planted) {
            Ok(()) => panic!("expected Err for plant `{expected_key}`, got Ok"),
            Err(e) => e,
        };
        assert!(
            err.contains(expected_key),
            "expected err to contain `{expected_key}`, got: {err}"
        );
    };

    // 1. "workers_dev": true (replace the false)
    let plant_workers_dev = content.replace("\"workers_dev\": false", "\"workers_dev\": true");
    assert_plant(&plant_workers_dev, "\"workers_dev\"");

    // 2. "preview_urls": true (replace the false)
    let plant_preview_urls = content.replace("\"preview_urls\": false", "\"preview_urls\": true");
    assert_plant(&plant_preview_urls, "\"preview_urls\"");

    // 3. a "routes": [{ "pattern": "x.example.invalid/*" }], entry inserted before the "compatibility_date" key
    let plant_routes = content.replace(
        "\"compatibility_date\"",
        "\"routes\": [{ \"pattern\": \"x.example.invalid/*\" }],\n  \"compatibility_date\"",
    );
    assert_plant(&plant_routes, "\"routes\"");

    // 4. a "account_id": "x", entry inserted likewise
    let plant_account_id = content.replace(
        "\"compatibility_date\"",
        "\"account_id\": \"x\",\n  \"compatibility_date\"",
    );
    assert_plant(&plant_account_id, "\"account_id\"");

    // 5. a "main": "src/index.js", entry inserted likewise
    let plant_main = content.replace(
        "\"compatibility_date\"",
        "\"main\": \"src/index.js\",\n  \"compatibility_date\"",
    );
    assert_plant(&plant_main, "\"main\"");

    // 6. the line containing "workers_dev": false deleted
    let plant_no_workers_dev = content
        .lines()
        .filter(|l| !l.contains("\"workers_dev\": false"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_plant(&plant_no_workers_dev, "\"workers_dev\"");

    // 7. the line containing "preview_urls": false deleted
    let plant_no_preview_urls = content
        .lines()
        .filter(|l| !l.contains("\"preview_urls\": false"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_plant(&plant_no_preview_urls, "\"preview_urls\"");

    // 8. "directory": "../out/hosted" replaced by "directory": "../out/edition"
    let plant_directory = content.replace(
        "\"directory\": \"../out/hosted\"",
        "\"directory\": \"../out/edition\"",
    );
    assert_plant(&plant_directory, "\"directory\"");
}

fn assert_no_workers_dev_subdomain(content: &str, rel: &str) {
    let target = ".workers.dev";
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find(target) {
        let abs_pos = search_from + pos;
        if abs_pos > 0
            && let Some(prev_char) = content[..abs_pos].chars().next_back()
        {
            assert!(
                !prev_char.is_ascii_alphanumeric() && prev_char != '-',
                "{rel}: found workers.dev subdomain pattern `{prev_char}{target}`"
            );
        }
        search_from = abs_pos + target.len();
    }
}

fn assert_no_32_hex_run(content: &str, rel: &str) {
    let mut current_run = 0;
    for ch in content.chars() {
        if ch.is_ascii_hexdigit() {
            current_run += 1;
        } else {
            assert!(
                current_run != 32,
                "{rel}: found run of exactly 32 hex digits"
            );
            current_run = 0;
        }
    }
    assert!(
        current_run != 32,
        "{rel}: found run of exactly 32 hex digits"
    );
}

#[test]
fn config_names_no_identifier() {
    let files = [
        "deploy/wrangler.jsonc",
        "deploy/_headers",
        "deploy/robots.txt",
    ];

    for rel in files {
        let path = repo_root().join(rel);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        assert!(!content.contains('@'), "{rel} must not contain '@'");
        assert!(
            !content.contains("cloudflareaccess.com"),
            "{rel} must not contain `cloudflareaccess.com`"
        );
        assert_no_workers_dev_subdomain(&content, rel);
        assert_no_32_hex_run(&content, rel);
    }
}
