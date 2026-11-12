//! Integration tests for supplied source text provenance notes.

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

use fieldbook_core::audit::Now;
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Host, Model, Options, Tree, load};
use fieldbook_core::date::CivilDate;
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::strings::EXTRACTION_SUPPLIED;
use fieldbook_render::meta::BuildInfo;
use fieldbook_render::site::{self, Site};

const CORPUS_TOML: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
const ASSISTANT_MD: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
const CHANGES_MD: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/changes.md");
const SOURCES_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
const CLAIMS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
const QUESTIONS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
const CONFLICTS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
const GLOSSARY_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
const S_0001_V1_TXT: &[u8] =
    include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt");
const S_0002_V2_TXT: &[u8] =
    include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0002-v2.txt");
const S_0001_V1_EXTRACTION_TOML: &str = include_str!(
    "../../../tests/fixtures/corpus-mini/corpus/sources/text/S-0001-v1/extraction.toml"
);
const S_0001_V1_P0001_TXT: &[u8] =
    include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/text/S-0001-v1/p0001.txt");
const CMP_SYSTEM_REVIEW_MD: &str = include_str!(
    "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
);

const SUPPLIED_EXTRACTION_TOML: &str = r#"tool = "supplied"
tool_version = "1"
method = "supplied"
page_count = 1
thin_pages = []
original_sha256 = "0fba4a465337f7383e693c1348ff428c4ce68dbfc6f6a3b5b64014d35d0eb73d"
[pages]
p0001 = "0fba4a465337f7383e693c1348ff428c4ce68dbfc6f6a3b5b64014d35d0eb73d"
[locators]
"1" = 1
"2" = 1
"#;

fn tree_with_extraction(
    extra_rows: &str,
    extra_files: &[(&str, &[u8])],
    extraction_toml: &str,
    p0001_bytes: &[u8],
) -> Tree {
    let mut tree = Tree::new();
    for dir in [
        "corpus",
        "corpus/eval",
        "corpus/registers",
        "corpus/sources",
        "corpus/sources/public",
        "corpus/sources/text",
        "corpus/sources/text/S-0001-v1",
        "corpus/pages",
        "corpus/pages/compliance",
        "corpus/pages/compliance/business-systems",
    ] {
        tree.insert_dir(dir);
    }
    tree.insert_bytes("corpus/corpus.toml", CORPUS_TOML.as_bytes());
    tree.insert_bytes("corpus/ASSISTANT.md", ASSISTANT_MD.as_bytes());
    tree.insert_bytes("corpus/changes.md", CHANGES_MD.as_bytes());
    tree.insert_bytes(
        "corpus/registers/sources.csv",
        format!("{SOURCES_CSV}{extra_rows}").as_bytes(),
    );
    tree.insert_bytes("corpus/registers/claims.csv", CLAIMS_CSV.as_bytes());
    tree.insert_bytes("corpus/registers/questions.csv", QUESTIONS_CSV.as_bytes());
    tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS_CSV.as_bytes());
    tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY_CSV.as_bytes());
    tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
    tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", S_0001_V1_TXT);
    tree.insert_bytes(
        "corpus/pages/compliance/business-systems/cmp-system-review.md",
        CMP_SYSTEM_REVIEW_MD.as_bytes(),
    );
    for (name, bytes) in extra_files {
        tree.insert_bytes(format!("corpus/sources/public/{name}"), *bytes);
    }
    tree.insert_bytes(
        "corpus/sources/text/S-0001-v1/extraction.toml",
        extraction_toml.as_bytes(),
    );
    tree.insert_bytes("corpus/sources/text/S-0001-v1/p0001.txt", p0001_bytes);
    tree
}

fn load_view(tree: Tree) -> (Model, LabelledView) {
    let today = CivilDate::parse("2026-03-01").unwrap();
    let (model, report) = load(&Host::new(tree, today), &Options::new(false));
    assert!(
        report.is_ok(),
        "tree must load cleanly:\n{}",
        report.render_text()
    );
    let eff = EffectivePolicy::floor(false);
    let now = Now::new(today, 0).unwrap();
    let Decision::Allowed(allowed) = check(&eff, &Dest::Edition, None, None, now, Shipped::Never)
    else {
        panic!("decision must be allowed");
    };
    let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
    (model, view)
}

fn render_site(view: &LabelledView) -> Site {
    site::render(view, &BuildInfo::default()).unwrap()
}

fn page_text(site: &Site, vid: &str) -> String {
    let path = format!("s/{vid}.html");
    String::from_utf8(
        site.files()
            .get(&path)
            .unwrap_or_else(|| panic!("missing {path} in site"))
            .clone(),
    )
    .unwrap()
}

#[test]
fn supplied_source_shows_the_note_below_the_title() {
    let tree = tree_with_extraction("", &[], SUPPLIED_EXTRACTION_TOML, S_0001_V1_P0001_TXT);
    let (_model, view) = load_view(tree);
    let site = render_site(&view);
    let html = page_text(&site, "S-0001-v1");

    assert!(
        html.contains(EXTRACTION_SUPPLIED),
        "page must contain the exact supplied-text provenance note"
    );
    assert_eq!(
        EXTRACTION_SUPPLIED,
        "text supplied by hand; not checked against the original"
    );

    let h1_idx = html.find("<h1").expect("h1 present");
    let note_idx = html.find(EXTRACTION_SUPPLIED).expect("note present");
    let panel_idx = html.find("class=\"panel").expect("panel present");
    assert!(h1_idx < note_idx, "note must appear after h1");
    assert!(note_idx < panel_idx, "note must appear before first panel");

    assert!(
        html.contains("<p class=\"muted supplied-note\" role=\"note\">"),
        "note element must have class muted supplied-note and role=note"
    );
}

#[test]
fn other_methods_show_no_supplied_note() {
    let tree = tree_with_extraction("", &[], S_0001_V1_EXTRACTION_TOML, S_0001_V1_P0001_TXT);
    let (_model, view) = load_view(tree);
    let site = render_site(&view);
    let html = page_text(&site, "S-0001-v1");

    assert!(
        !html.contains(EXTRACTION_SUPPLIED),
        "plain extraction must not contain supplied note text"
    );
    assert!(
        !html.contains("supplied-note"),
        "plain extraction must not contain supplied-note class"
    );
}

#[test]
fn supplied_note_is_neutral() {
    let tree = tree_with_extraction("", &[], SUPPLIED_EXTRACTION_TOML, S_0001_V1_P0001_TXT);
    let (_model, view) = load_view(tree);
    let site = render_site(&view);
    let html = page_text(&site, "S-0001-v1");

    let note_pos = html.find("supplied-note").expect("supplied-note present");
    let tag_start = html[..note_pos].rfind('<').expect("< before note");
    let tag_end = html[note_pos..].find('>').expect("> after note") + note_pos;
    let note_tag = &html[tag_start..=tag_end];

    for forbidden in ["warn", "bad", "ok", "accent"] {
        assert!(
            !note_tag.contains(forbidden),
            "note tag {note_tag} must not contain class {forbidden}"
        );
    }
    assert!(
        !note_tag.contains("style="),
        "note tag {note_tag} must not contain style attribute"
    );
    assert!(
        !html.contains("style=\""),
        "whole page must not contain style attribute"
    );
}
