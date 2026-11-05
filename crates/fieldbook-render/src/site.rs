//! Site container and static edition renderer (`build-v0`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use askama::Template;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::VersionId;
use fieldbook_core::manifest::shipped_corpus;
use fieldbook_core::schema::vocab::Lifecycle;
use fieldbook_core::strings;
use fieldbook_core::version::SCHEMA_VERSION_MAX;

use crate::csp::CSP_STATIC;
use crate::meta::{BuildInfo, MetadataError, RenderContext};
use fieldbook_core::edition::latest_edition_heading;

const APP_CSS: &str = include_str!("../assets/app.css");
const PRODUCT_TITLE: &str = "Fieldbook";

/// In-memory map of relative output paths (`/`-separated) to deterministic file bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Site(BTreeMap<String, Vec<u8>>);

impl Site {
    /// Creates an empty [`Site`].
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Inserts `bytes` at `path`.
    ///
    /// `path` must be relative with no `..` component, no leading `/`, and no backslashes.
    /// Inserting a duplicate path is a bug and triggers a debug assertion.
    pub fn insert(&mut self, path: &str, bytes: Vec<u8>) {
        debug_assert!(
            is_valid_relative_site_path(path),
            "site path must be relative with no `..` or leading `/`: {path}"
        );
        let prev = self.0.insert(path.to_owned(), bytes);
        debug_assert!(prev.is_none(), "duplicate site path inserted: {path}");
    }

    /// Returns the map of relative output paths to file bytes.
    #[must_use]
    pub const fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.0
    }
}

fn is_valid_relative_site_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return false;
    }
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

#[derive(Template)]
#[template(path = "base.html")]
struct BaseTemplate<'a> {
    csp: &'a str,
    product: &'a str,
    root_prefix: &'a str,
    built_date: Option<&'a str>,
    unreleased_build: &'a str,
}

/// Validates and constructs the [`RenderContext`] for a public edition build.
///
/// # Errors
///
/// Returns [`MetadataError`] if `view` is non-public or if `changes.md` fails edition metadata checks.
pub fn render_context(
    view: &LabelledView,
    info: &BuildInfo,
) -> Result<RenderContext, MetadataError> {
    let mut quarantined = BTreeSet::new();
    for s in view.sources() {
        if s.value.lifecycle == Lifecycle::Quarantined
            && let Ok(vid) =
                VersionId::parse(&format!("{}-v{}", s.value.source_id, s.value.version))
        {
            quarantined.insert(vid);
        }
    }
    let files = shipped_corpus(view, &quarantined)?;
    let heading = view
        .tree()
        .get_bytes("corpus/changes.md")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(latest_edition_heading);
    let schema_version = view
        .corpus()
        .map_or(SCHEMA_VERSION_MAX, |c| c.schema_version);
    RenderContext::public(
        view,
        &files,
        heading,
        info,
        env!("CARGO_PKG_VERSION"),
        schema_version,
    )
}

/// Returns the deterministic list of relative paths planned for `ctx`.
#[must_use]
pub fn planned_paths(ctx: &RenderContext) -> Vec<String> {
    match ctx {
        RenderContext::Public(_edition) => vec![
            "assets/app.css".to_owned(),
            "assets/strings.js".to_owned(),
            "index.html".to_owned(),
        ],
    }
}

/// Renders a [`Site`] from a [`RenderContext`].
#[must_use]
pub fn render_with_context(ctx: &RenderContext) -> Site {
    let built_date = match ctx {
        RenderContext::Public(edition) => {
            (!edition.built.is_empty()).then_some(edition.built.as_str())
        }
    };
    let index_tpl = BaseTemplate {
        csp: CSP_STATIC,
        product: PRODUCT_TITLE,
        root_prefix: "",
        built_date,
        unreleased_build: strings::unreleased_build(),
    };
    let index_html = index_tpl
        .render()
        .unwrap_or_else(|err| unreachable!("infallible base.html template rendering: {err}"));

    let mut site = Site::new();
    site.insert("index.html", index_html.into_bytes());
    site.insert("assets/strings.js", strings::js().into_bytes());
    site.insert("assets/app.css", APP_CSS.as_bytes().to_vec());
    debug_assert_eq!(
        site.files().keys().cloned().collect::<Vec<_>>(),
        planned_paths(ctx)
    );
    site
}

/// Renders the static edition [`Site`] from `view` and `info`.
///
/// Reads no clock or unrestricted model. Validates [`RenderContext::public`] first.
///
/// # Errors
///
/// Returns [`MetadataError`] if `view` is non-public or if `changes.md` fails edition metadata checks.
pub fn render(view: &LabelledView, info: &BuildInfo) -> Result<Site, MetadataError> {
    let ctx = render_context(view, info)?;
    Ok(render_with_context(&ctx))
}
