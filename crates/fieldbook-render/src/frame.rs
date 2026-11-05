//! Global page frame (`AppShell`, `TopBar`, `Sidebar`, `CrumbsRow`, `Banner`, `Toast`, `StoreContext`).

use askama::Template;
use fieldbook_core::audit::LabelSet;
use fieldbook_core::strings;

use crate::csp::CSP_STATIC;
use crate::nav::{NavTarget, Navigation, SectionId, resolve_route};
use crate::site::PlannedPaths;

/// Product name rendered in the brand header and default page `<title>`.
pub const PRODUCT_TITLE: &str = "Fieldbook";

/// Explicit store context and effective-label presentation input (`build-frame` step 9).
///
/// Never inferred from an item label; the public edition passes [`Self::Public`], and the
/// later company overlay build passes [`Self::Company`] with its effective label snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreContext {
    /// Public edition (`Public data only. Company data is not connected.`).
    Public,
    /// Company storage view with its effective handling label snapshot.
    Company {
        /// Effective handling labels governing this local view.
        effective_labels: LabelSet,
    },
}

impl StoreContext {
    fn formatted_labels(labels: &LabelSet) -> String {
        labels
            .ids()
            .iter()
            .map(fieldbook_core::audit::LabelId::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn presentation(&self) -> StorePresentation {
        match self {
            Self::Public => StorePresentation {
                chip_label: "Public data only".to_owned(),
                chip_title:
                    "This build reads the public library only. Company data is not connected."
                        .to_owned(),
                foot_lead: "Public data only.".to_owned(),
                foot_body: "Company data is not connected.".to_owned(),
                banner: None,
            },
            Self::Company { effective_labels } => {
                let joined = Self::formatted_labels(effective_labels);
                StorePresentation {
                    chip_label: "Company storage".to_owned(),
                    chip_title: format!("Company storage. Handling: {joined}."),
                    foot_lead: "Company storage.".to_owned(),
                    foot_body: format!("Handling: {joined}."),
                    banner: Some(StoreBannerView {
                        lead: format!("Handling: {joined}."),
                        body: "Company storage. Local view; no outward permission is implied."
                            .to_owned(),
                    }),
                }
            }
        }
    }
}

struct StorePresentation {
    chip_label: String,
    chip_title: String,
    foot_lead: String,
    foot_body: String,
    banner: Option<StoreBannerView>,
}

struct StoreBannerView {
    lead: String,
    body: String,
}

/// One breadcrumb item in `CrumbsRow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    /// Visible breadcrumb text.
    pub label: String,
    /// Relative destination from the edition root if built, or `None` for the current/unbuilt item.
    pub href: Option<String>,
}

impl Crumb {
    /// Creates a breadcrumb that links to `href` only when `href` is in `planned`.
    #[must_use]
    pub fn linked(
        label: impl Into<String>,
        href: impl Into<String>,
        planned: &PlannedPaths,
    ) -> Self {
        let raw_href = href.into();
        let file_part = raw_href.split('#').next().unwrap_or(&raw_href);
        let resolved = (!file_part.is_empty() && planned.contains(file_part)).then_some(raw_href);
        Self {
            label: label.into(),
            href: resolved,
        }
    }

    /// Creates the terminal (current-page) breadcrumb with no link.
    #[must_use]
    pub fn current(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: None,
        }
    }
}

/// Computes the relative prefix (`""`, `"../"`, `"../../"`, ...) from `output_path` back to the edition root.
#[must_use]
pub fn root_prefix_for_path(output_path: &str) -> String {
    let clean = output_path
        .split('#')
        .next()
        .unwrap_or(output_path)
        .trim_matches('/');
    let depth = clean.bytes().filter(|&b| b == b'/').count();
    "../".repeat(depth)
}

/// Inputs required to render one HTML page inside the global frame.
#[derive(Debug, Clone)]
pub struct FrameInput<'a> {
    /// Relative output path of this page from the edition root (e.g. `"index.html"`, `"p/fnd-0001.html"`).
    pub output_path: &'a str,
    /// Document `<title>`.
    pub title: &'a str,
    /// Optional explicit area code for `p/<page-id>.html` routes.
    pub page_area: Option<&'a str>,
    /// Release build date (`YYYY-MM-DD`) when `BuildInfo::is_released` holds, or `None`.
    pub built_date: Option<&'a str>,
    /// Public vs. company store context.
    pub store: &'a StoreContext,
    /// Breadcrumb trail; rendered only when `crumbs.len() >= 2`.
    pub crumbs: &'a [Crumb],
    /// Pre-rendered HTML placed inside `<main id="main">`.
    pub main_html: &'a str,
    /// Navigation model built from [`LabelledView`] and [`PlannedPaths`].
    pub nav: &'a Navigation,
}

struct TabView<'a> {
    key: &'a str,
    label: &'a str,
    href: Option<&'a str>,
    landing_line: &'a str,
    is_current: bool,
}

struct EntryView<'a> {
    id: &'a str,
    code: Option<&'a str>,
    label: &'a str,
    sub_group: Option<&'a str>,
    href: Option<&'a str>,
    landing_line: &'a str,
    count: Option<usize>,
    is_current: bool,
}

struct GroupView<'a> {
    key: &'a str,
    label: &'a str,
    aria_label: &'a str,
    is_current: bool,
    entries: Vec<EntryView<'a>>,
}

#[derive(Template)]
#[template(path = "base.html")]
struct BaseTemplate<'a> {
    csp: &'a str,
    product: &'a str,
    title: &'a str,
    output_path: &'a str,
    root_prefix: &'a str,
    current_section_key: &'a str,
    current_item_id: &'a str,
    built_date: Option<&'a str>,
    unreleased_build: &'a str,
    store_chip_label: &'a str,
    store_chip_title: &'a str,
    store_foot_lead: &'a str,
    store_foot_body: &'a str,
    store_banner: Option<StoreBannerView>,
    tabs: Vec<TabView<'a>>,
    groups: Vec<GroupView<'a>>,
    crumbs: &'a [Crumb],
    main_html: &'a str,
}

/// Renders one HTML document wrapped in the global frame (`base.html`).
#[must_use]
pub fn render_page(input: &FrameInput<'_>) -> String {
    let (current_section, current_item) = resolve_route(input.output_path, input.page_area)
        .unwrap_or((SectionId::Today, "today".to_owned()));
    let root_prefix = root_prefix_for_path(input.output_path);
    let store_pres = input.store.presentation();

    let tabs = input
        .nav
        .tabs
        .iter()
        .map(|t| {
            let (href, landing_line) = match &t.target {
                NavTarget::Built { href } => (Some(href.as_str()), ""),
                NavTarget::Unbuilt { landing_line } => (None, landing_line.as_str()),
            };
            TabView {
                key: t.key,
                label: t.label,
                href,
                landing_line,
                is_current: t.id == current_section,
            }
        })
        .collect();

    let groups = input
        .nav
        .groups
        .iter()
        .map(|g| {
            let entries = g
                .entries
                .iter()
                .map(|e| {
                    let (href, landing_line) = match &e.target {
                        NavTarget::Built { href } => (Some(href.as_str()), ""),
                        NavTarget::Unbuilt { landing_line } => (None, landing_line.as_str()),
                    };
                    EntryView {
                        id: e.id.as_str(),
                        code: e.code.as_deref(),
                        label: e.label.as_str(),
                        sub_group: e.sub_group.as_deref(),
                        href,
                        landing_line,
                        count: e.count,
                        is_current: g.id == current_section && e.id == current_item,
                    }
                })
                .collect();
            GroupView {
                key: g.key,
                label: g.id.label(),
                aria_label: g.aria_label,
                is_current: g.id == current_section,
                entries,
            }
        })
        .collect();

    let tpl = BaseTemplate {
        csp: CSP_STATIC,
        product: PRODUCT_TITLE,
        title: input.title,
        output_path: input.output_path,
        root_prefix: &root_prefix,
        current_section_key: current_section.key(),
        current_item_id: &current_item,
        built_date: input.built_date,
        unreleased_build: strings::unreleased_build(),
        store_chip_label: &store_pres.chip_label,
        store_chip_title: &store_pres.chip_title,
        store_foot_lead: &store_pres.foot_lead,
        store_foot_body: &store_pres.foot_body,
        store_banner: store_pres.banner,
        tabs,
        groups,
        crumbs: input.crumbs,
        main_html: input.main_html,
    };

    tpl.render()
        .unwrap_or_else(|err| unreachable!("infallible base.html template rendering: {err}"))
}
