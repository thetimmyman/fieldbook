//! Site container and static edition renderer (`build-v0`, `build-frame`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use fieldbook_core::edition::latest_edition_heading;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::VersionId;
use fieldbook_core::manifest::shipped_corpus;
use fieldbook_core::schema::Register;
use fieldbook_core::schema::vocab::Lifecycle;
use fieldbook_core::strings;
use fieldbook_core::version::SCHEMA_VERSION_MAX;

use crate::cite::OriginalAvailability;
use crate::meta::{BuildInfo, BuildProfile, MetadataError, RenderContext};
use crate::{
    about, area, day_one, edition, home, ids, nav, register, search_index, search_page, source,
};

const APP_CSS: &str = include_str!("../assets/app.css");
/// `assets/app.js`, which the harness also loads into its shortcut pages.
pub const APP_JS: &str = include_str!("../assets/app.js");

/// Deterministic set of relative output paths planned for a build (`build-frame`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlannedPaths(BTreeSet<String>);

impl PlannedPaths {
    /// Builds a [`PlannedPaths`] set from a slice of path literals (e.g. for unit/snapshot tests).
    #[must_use]
    pub fn of(paths: &[&str]) -> Self {
        Self(paths.iter().map(|&p| p.to_owned()).collect())
    }

    /// Builds a [`PlannedPaths`] set from owned path strings.
    #[must_use]
    pub fn from_owned(paths: impl IntoIterator<Item = String>) -> Self {
        Self(paths.into_iter().collect())
    }

    /// Returns `true` when `path` is in the planned output set.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.0.contains(path)
    }

    /// Iterates over the planned output paths in deterministic lexicographical order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

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

/// Returns the deterministic set of relative output paths planned for `view`.
#[must_use]
pub fn planned_paths(view: &LabelledView) -> PlannedPaths {
    let mut paths = vec![
        "assets/app.css".to_owned(),
        "assets/app.js".to_owned(),
        search_index::OUTPUT_PATH.to_owned(),
        search_page::SCRIPT_PATH.to_owned(),
        "assets/strings.js".to_owned(),
        search_page::OUTPUT_PATH.to_owned(),
        ids::OUTPUT_PATH.to_owned(),
    ];
    paths.extend(ids::page_paths(view));
    paths.extend([
        home::OUTPUT_PATH.to_owned(),
        about::OUTPUT_PATH.to_owned(),
        day_one::OUTPUT_PATH.to_owned(),
        edition::OUTPUT_PATH.to_owned(),
    ]);
    paths.extend(area::page_paths(view));
    for reg in Register::ALL {
        paths.push(register::output_path(reg).to_owned());
    }
    paths.extend(register::claims_part_paths(view));
    for page in view.pages() {
        paths.push(format!("p/{}.html", page.id));
    }
    for vid in source::page_versions(view) {
        paths.push(source::page_path(vid.as_str()));
    }
    PlannedPaths::from_owned(paths)
}

/// Renders a [`Site`] from `view`, a validated [`RenderContext`] and the compiled `profile`.
#[must_use]
pub fn render_with_context(
    view: &LabelledView,
    ctx: &RenderContext,
    profile: BuildProfile,
) -> Site {
    let planned = planned_paths(view);
    let navigation = nav::build(view, &planned);
    let mut site = Site::new();
    let index_html = home::render_page(view, ctx, &navigation, &planned);
    site.insert(home::OUTPUT_PATH, index_html.into_bytes());
    if let Some(corpus) = view.corpus() {
        for def in &corpus.area {
            let html = area::render_page(def, view, ctx, &navigation, &planned);
            site.insert(&area::page_path(&def.code), html.into_bytes());
        }
    }
    for reg in Register::ALL {
        let reg_path = register::output_path(reg);
        let html = register::render_page(reg, view, ctx, &navigation, &planned);
        site.insert(reg_path, html.into_bytes());
    }
    let parts = register::claims_parts(view);
    for (i, part) in parts.iter().enumerate() {
        let html = register::render_claims_part_page(&parts, i, view, ctx, &navigation, &planned);
        site.insert(&part.route, html.into_bytes());
    }
    for (path, html) in ids::render_all(view, ctx, &navigation, &planned) {
        site.insert(&path, html.into_bytes());
    }
    let about_html = about::render_page(view, ctx, profile, &navigation, &planned);
    site.insert(about::OUTPUT_PATH, about_html.into_bytes());
    site.insert(
        day_one::OUTPUT_PATH,
        day_one::render_page(view, ctx, &navigation, &planned).into_bytes(),
    );
    site.insert(edition::OUTPUT_PATH, edition::toml_bytes(ctx));
    let packs = fieldbook_core::assist::PackSource::new(view);
    let links = crate::related::LinkIndex::new(view);
    for page in view.pages() {
        let page_path = format!("p/{}.html", page.id);
        let html = crate::page::render_page_with_packs(
            page,
            view,
            ctx,
            &navigation,
            &planned,
            &packs,
            &links,
        );
        site.insert(&page_path, html.into_bytes());
    }
    let availability = OriginalAvailability::from_view(view);
    for vid in source::page_versions(view) {
        if let Some(html) =
            source::render_page(&vid, view, ctx, &navigation, &planned, &availability)
        {
            site.insert(&source::page_path(vid.as_str()), html.into_bytes());
        }
    }
    let search = search_index::build(view);
    let search_html = search_page::render_page(&search, ctx, &navigation, &planned);
    site.insert(search_page::OUTPUT_PATH, search_html.into_bytes());
    site.insert(
        search_index::OUTPUT_PATH,
        search_index::write(&search).into_bytes(),
    );
    site.insert(
        search_page::SCRIPT_PATH,
        search_page::SCRIPT.as_bytes().to_vec(),
    );
    site.insert("assets/strings.js", strings::js().into_bytes());
    site.insert("assets/app.css", APP_CSS.as_bytes().to_vec());
    site.insert("assets/app.js", APP_JS.as_bytes().to_vec());
    debug_assert_eq!(
        site.files().keys().map(String::as_str).collect::<Vec<_>>(),
        planned.iter().collect::<Vec<_>>()
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
    Ok(render_with_context(view, &ctx, info.profile))
}
