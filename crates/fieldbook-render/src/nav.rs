//! Global frame navigation model and route-to-section mapping (`build-frame`).

use fieldbook_core::egress::LabelledView;
use fieldbook_core::strings;

use crate::meta::BuildStatus;
use crate::site::PlannedPaths;

/// `index.html#areas` is built (`build-home-and-areas`): All areas and Browse are aliases to it.
pub const AREAS_ANCHOR_BUILT: bool = true;

/// The six top-level sections of the global frame (`HANDOFF.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SectionId {
    /// Today (`today`).
    Today,
    /// Assistant (`assistant`).
    Assistant,
    /// Work (`work`).
    Work,
    /// Plan (`plan`).
    Plan,
    /// Areas (`areas`).
    Areas,
    /// Library (`library`).
    Library,
}

impl SectionId {
    /// All six sections in display order.
    pub const ALL: [Self; 6] = [
        Self::Today,
        Self::Assistant,
        Self::Work,
        Self::Plan,
        Self::Areas,
        Self::Library,
    ];

    /// Stable lowercase key used in `data-sec`, `data-secpick`, and `app.js`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Assistant => "assistant",
            Self::Work => "work",
            Self::Plan => "plan",
            Self::Areas => "areas",
            Self::Library => "library",
        }
    }

    /// Display label in the top bar and section switch.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Assistant => "Assistant",
            Self::Work => "Work",
            Self::Plan => "Plan",
            Self::Areas => "Areas",
            Self::Library => "Library",
        }
    }

    /// Accessible label for the `<nav class="nav">` inside this section's sidebar group.
    #[must_use]
    pub const fn nav_aria_label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Assistant => "Assistant",
            Self::Work => "Work",
            Self::Plan => "90-day plan",
            Self::Areas => "Areas",
            Self::Library => "Library",
        }
    }

    /// Parses a lowercase section key (`"today"`, `"assistant"`, etc.).
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }

    /// The `(thing, release_or_trigger)` pair used when the section's first page is not built.
    #[must_use]
    pub const fn landing(self) -> (&'static str, &'static str) {
        match self {
            Self::Today => ("Today", "Day 1"),
            Self::Assistant => ("The assistant", "Day 30"),
            Self::Work => ("Bids", "the ORG overlay, Release 2 or later"),
            Self::Plan => ("The 90-day plan", "Day 1"),
            Self::Areas => ("Areas", "Day 1"),
            Self::Library => ("The library", "Day 1"),
        }
    }

    /// Primary output path for the section tab, if any.
    #[must_use]
    pub fn first_path(self) -> Option<&'static str> {
        match self {
            Self::Today => Some("index.html"),
            Self::Assistant => Some(strings::paths()[4]),
            Self::Work | Self::Plan => None,
            Self::Areas | Self::Library => AREAS_ANCHOR_BUILT.then_some("index.html#areas"),
        }
    }
}

/// One row of the canonical route-to-section table (`AD-12` output paths).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteMapping {
    /// Route pattern (`"index.html"`, `"a/<area>.html"`, `"p/<page-id>.html"`, etc.).
    pub route: &'static str,
    /// Section that owns the route.
    pub section: SectionId,
    /// Sidebar item id marked current (`"today"`, `"area-<area>"`, `"claims"`, etc.).
    pub item: &'static str,
}

/// Returns the canonical route-to-section mapping for all AD-12 HTML output paths.
#[must_use]
pub fn route_sections() -> Vec<RouteMapping> {
    let map_html = strings::paths()[5];
    let assistant_html = strings::paths()[4];
    let day_one_html = strings::paths()[3];
    vec![
        RouteMapping {
            route: "index.html",
            section: SectionId::Today,
            item: "today",
        },
        RouteMapping {
            route: "index.html#areas",
            section: SectionId::Today,
            item: "today",
        },
        RouteMapping {
            route: map_html,
            section: SectionId::Today,
            item: "map",
        },
        RouteMapping {
            route: "about.html",
            section: SectionId::Today,
            item: "today",
        },
        RouteMapping {
            route: "changes.html",
            section: SectionId::Today,
            item: "today",
        },
        RouteMapping {
            route: assistant_html,
            section: SectionId::Assistant,
            item: "assistant",
        },
        RouteMapping {
            route: "a/<area>.html",
            section: SectionId::Areas,
            item: "area-<area>",
        },
        RouteMapping {
            route: "p/<page-id>.html",
            section: SectionId::Areas,
            item: "area-<area>",
        },
        RouteMapping {
            route: day_one_html,
            section: SectionId::Library,
            item: "templates",
        },
        RouteMapping {
            route: "claims.html",
            section: SectionId::Library,
            item: "claims",
        },
        RouteMapping {
            route: "questions.html",
            section: SectionId::Library,
            item: "questions",
        },
        RouteMapping {
            route: "conflicts.html",
            section: SectionId::Library,
            item: "conflicts",
        },
        RouteMapping {
            route: "glossary.html",
            section: SectionId::Library,
            item: "glossary",
        },
        RouteMapping {
            route: "sources.html",
            section: SectionId::Library,
            item: "sources",
        },
        RouteMapping {
            route: "s/<version-id>.html",
            section: SectionId::Library,
            item: "sources",
        },
        RouteMapping {
            route: "ids.html",
            section: SectionId::Library,
            item: "browse",
        },
        RouteMapping {
            route: "search.html",
            section: SectionId::Library,
            item: "browse",
        },
    ]
}

/// Resolves a concrete output path (e.g. `"index.html"`, `"a/fnd.html"`, `"p/fnd-0001-x.html"`)
/// to its `(SectionId, current_item_id)` pair.
///
/// For `p/<page-id>.html`, `page_area` supplies the area code if not inferable from `<page-id>`.
#[must_use]
pub fn resolve_route(path: &str, page_area: Option<&str>) -> Option<(SectionId, String)> {
    let clean = path.split('#').next().unwrap_or(path);
    if let Some(area) = clean
        .strip_prefix("a/")
        .and_then(|s| s.strip_suffix(".html"))
        && !area.is_empty()
        && !area.contains('/')
    {
        return Some((
            SectionId::Areas,
            format!("area-{}", area.to_ascii_lowercase()),
        ));
    }
    if let Some(stem) = clean
        .strip_prefix("p/")
        .and_then(|s| s.strip_suffix(".html"))
        && !stem.is_empty()
        && !stem.contains('/')
    {
        let area = page_area
            .or_else(|| stem.split('-').next())
            .unwrap_or("fnd")
            .to_ascii_lowercase();
        return Some((SectionId::Areas, format!("area-{area}")));
    }
    if let Some(ver) = clean
        .strip_prefix("s/")
        .and_then(|s| s.strip_suffix(".html"))
        && !ver.is_empty()
        && !ver.contains('/')
    {
        return Some((SectionId::Library, "sources".to_owned()));
    }
    // A claims part page; `app.js` (at its size budget) keeps no map row for it, because the
    // frame sets `data-section` and `data-item` from this function.
    if let Some(part) = clean
        .strip_prefix("claims/")
        .and_then(|s| s.strip_suffix(".html"))
        && !part.is_empty()
        && !part.contains('/')
    {
        return Some((SectionId::Library, "claims".to_owned()));
    }
    for row in route_sections() {
        if row.route == clean {
            return Some((row.section, row.item.to_owned()));
        }
    }
    None
}

/// Formats `strings::not_available()` by substituting the thing and release/trigger placeholders.
#[must_use]
pub fn format_not_available(thing: &str, until: &str) -> String {
    strings::not_available()
        .replace("<Thing>", thing)
        .replace("<release or trigger>", until)
}

/// Destination state of a section tab or sidebar entry after checking [`PlannedPaths`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    /// Target file is built in [`PlannedPaths`]; renders as `<a href="...">`.
    Built {
        /// Relative path from the edition root (e.g. `"index.html"`, `"a/fnd.html"`).
        href: String,
    },
    /// Target file is not in [`PlannedPaths`]; renders as `.not-available` text with the landing line.
    Unbuilt {
        /// Formatted `not_available` sentence.
        landing_line: String,
    },
}

impl NavTarget {
    fn from_candidate(
        path: Option<&str>,
        thing: &str,
        until: &str,
        planned: &PlannedPaths,
    ) -> Self {
        if let Some(href) = path {
            let file_part = href.split('#').next().unwrap_or(href);
            if !file_part.is_empty() && planned.contains(file_part) {
                return Self::Built {
                    href: href.to_owned(),
                };
            }
        }
        Self::Unbuilt {
            landing_line: format_not_available(thing, until),
        }
    }
}

/// Resolved top-bar section tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionTab {
    /// Section identifier.
    pub id: SectionId,
    /// Lowercase key (`"today"`, `"assistant"`, etc.).
    pub key: &'static str,
    /// Display label (`"Today"`, `"Assistant"`, etc.).
    pub label: &'static str,
    /// Built link or unbuilt landing line.
    pub target: NavTarget,
}

/// Resolved sidebar entry inside a section's `ContextNav`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavEntry {
    /// Item identifier (`"today"`, `"map"`, `"area-fnd"`, `"claims"`, etc.).
    pub id: String,
    /// Optional 3-letter uppercase code badge (`"FND"`, `"CPY"`, etc.).
    pub code: Option<String>,
    /// Display label.
    pub label: String,
    /// Optional sub-group heading rendered immediately above this entry (`"Views over MKT"`).
    pub sub_group: Option<String>,
    /// Built link or unbuilt landing line.
    pub target: NavTarget,
    /// Item count from [`LabelledView`], or `None` for entries without a count.
    pub count: Option<usize>,
}

/// Resolved sidebar section group (`<div class="nav-group" data-sec="...">`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionGroup {
    /// Section identifier.
    pub id: SectionId,
    /// Lowercase section key.
    pub key: &'static str,
    /// Accessible label for `<nav class="nav">`.
    pub aria_label: &'static str,
    /// Entries in display order.
    pub entries: Vec<NavEntry>,
}

/// Complete navigation data for a build, constructed from [`LabelledView`] and [`PlannedPaths`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation {
    /// The six top-bar section tabs in order.
    pub tabs: Vec<SectionTab>,
    /// The six sidebar section groups in order.
    pub groups: Vec<SectionGroup>,
    /// Build-time claim counts for the unreleased build status banner.
    pub build_status: crate::meta::BuildStatus,
}

impl Navigation {
    /// Returns the sidebar group for `section`.
    #[must_use]
    pub fn group(&self, section: SectionId) -> Option<&SectionGroup> {
        self.groups.iter().find(|g| g.id == section)
    }

    /// Returns the entry with `item_id` across all groups.
    #[must_use]
    pub fn entry(&self, item_id: &str) -> Option<&NavEntry> {
        self.groups
            .iter()
            .flat_map(|g| &g.entries)
            .find(|e| e.id == item_id)
    }
}

/// Builds the complete [`Navigation`] model from the filtered `view` and `planned` output paths.
///
/// - Counts are computed strictly from `view` (`pages`, `claims`, `questions`, `conflicts`,
///   `glossary`, `sources`).
/// - Links are emitted only for paths present in `planned`; all other entries and tabs carry
///   [`NavTarget::Unbuilt`] with their formatted `not_available` landing line.
#[must_use]
pub fn build(view: &LabelledView, planned: &PlannedPaths) -> Navigation {
    let map_html = strings::paths()[5];
    let assistant_html = strings::paths()[4];
    let day_one_html = strings::paths()[3];
    let sources_label = strings::parts()[6];
    let areas_alias_path = AREAS_ANCHOR_BUILT.then_some("index.html#areas");

    let today_entries = vec![
        NavEntry {
            id: "today".to_owned(),
            code: None,
            label: "Today".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(Some("index.html"), "Today", "Day 1", planned),
            count: None,
        },
        NavEntry {
            id: "map".to_owned(),
            code: None,
            label: "App map".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(Some(map_html), "The app map", "Day 30", planned),
            count: None,
        },
    ];

    let assistant_entries = vec![
        NavEntry {
            id: "assistant".to_owned(),
            code: None,
            label: "Workspace".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                Some(assistant_html),
                "The assistant",
                "Day 30",
                planned,
            ),
            count: None,
        },
        NavEntry {
            id: "handoff".to_owned(),
            code: None,
            label: "Handoff packs".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                Some(assistant_html),
                "Handoff packs",
                "Day 30",
                planned,
            ),
            count: None,
        },
    ];

    let work_specs = [
        ("bids", "Bids"),
        ("programs", "Programs"),
        ("close", "Close & forecast"),
        ("rates", "Rates & pools"),
        ("suppliers", "Suppliers & make/buy"),
        ("compliance", "Systems & compliance"),
        ("mna", "M&A & cases"),
        ("actions", "Actions & calendar"),
    ];
    let work_entries = work_specs
        .into_iter()
        .map(|(id, label)| NavEntry {
            id: id.to_owned(),
            code: None,
            label: label.to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                None,
                label,
                "the ORG overlay, Release 2 or later",
                planned,
            ),
            count: None,
        })
        .collect();

    let plan_specs = [
        ("plan-90", "90-day plan", "The 90-day plan", "Day 1"),
        ("curriculum", "Curriculum", "The curriculum", "Day 1"),
        (
            "stakeholders",
            "Stakeholders",
            "Stakeholders",
            "the ORG overlay, Release 2 or later",
        ),
        (
            "systems",
            "Systems",
            "Systems",
            "the ORG overlay, Release 2 or later",
        ),
        (
            "artifacts",
            "Artifact log",
            "The artifact log",
            "the ORG overlay, Release 2 or later",
        ),
    ];
    let plan_entries = plan_specs
        .into_iter()
        .map(|(id, label, thing, until)| NavEntry {
            id: id.to_owned(),
            code: None,
            label: label.to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(None, thing, until, planned),
            count: None,
        })
        .collect();

    let mut areas_entries = Vec::new();
    areas_entries.push(NavEntry {
        id: "areas".to_owned(),
        code: None,
        label: "All areas".to_owned(),
        sub_group: None,
        target: NavTarget::from_candidate(areas_alias_path, "Areas", "Day 1", planned),
        count: None,
    });

    if let Some(corpus) = view.corpus() {
        for area in &corpus.area {
            let code_lower = area.code.to_ascii_lowercase();
            let path = format!("a/{code_lower}.html");
            let count = view
                .pages()
                .iter()
                .filter(|p| p.meta.area.as_str().eq_ignore_ascii_case(&area.code))
                .count();
            areas_entries.push(NavEntry {
                id: format!("area-{code_lower}"),
                code: Some(area.code.to_ascii_uppercase()),
                label: area.name.clone(),
                sub_group: None,
                target: NavTarget::from_candidate(Some(&path), &area.name, "Day 1", planned),
                count: Some(count),
            });
        }

        for (idx, v) in corpus.view.iter().enumerate() {
            let code_lower = v.code.to_ascii_lowercase();
            let count = view
                .pages()
                .iter()
                .filter(|p| {
                    p.meta.area.as_str().eq_ignore_ascii_case(&v.filter.area)
                        && p.meta.subarea.eq_ignore_ascii_case(&v.filter.subarea)
                })
                .count();
            let sub_group =
                (idx == 0).then(|| format!("Views over {}", v.filter.area.to_ascii_uppercase()));
            areas_entries.push(NavEntry {
                id: format!("view-{code_lower}"),
                code: Some(v.code.to_ascii_uppercase()),
                label: v.name.clone(),
                sub_group,
                target: NavTarget::from_candidate(None, &v.name, "Day 30", planned),
                count: Some(count),
            });
        }
    }

    let library_entries = vec![
        NavEntry {
            id: "browse".to_owned(),
            code: None,
            label: "Browse".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(areas_alias_path, "The library", "Day 1", planned),
            count: None,
        },
        NavEntry {
            id: "templates".to_owned(),
            code: None,
            label: "Templates & tools".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                Some(day_one_html),
                "The day-one binder",
                "Day 1",
                planned,
            ),
            count: None,
        },
        NavEntry {
            id: "claims".to_owned(),
            code: None,
            label: "Claims".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                Some("claims.html"),
                "The claims register",
                "Day 1",
                planned,
            ),
            count: Some(view.claims().len()),
        },
        NavEntry {
            id: "questions".to_owned(),
            code: None,
            label: "Questions".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                Some("questions.html"),
                "The questions register",
                "Day 1",
                planned,
            ),
            count: Some(view.questions().len()),
        },
        NavEntry {
            id: "conflicts".to_owned(),
            code: None,
            label: "Conflicts".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                Some("conflicts.html"),
                "The conflicts register",
                "Day 1",
                planned,
            ),
            count: Some(view.conflicts().len()),
        },
        NavEntry {
            id: "glossary".to_owned(),
            code: None,
            label: "Glossary".to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                Some("glossary.html"),
                "The glossary",
                "Day 1",
                planned,
            ),
            count: Some(view.glossary().len()),
        },
        NavEntry {
            id: "sources".to_owned(),
            code: None,
            label: sources_label.to_owned(),
            sub_group: None,
            target: NavTarget::from_candidate(
                Some("sources.html"),
                "The sources register",
                "Day 1",
                planned,
            ),
            count: Some(view.sources().len()),
        },
    ];

    let groups = vec![
        SectionGroup {
            id: SectionId::Today,
            key: SectionId::Today.key(),
            aria_label: SectionId::Today.nav_aria_label(),
            entries: today_entries,
        },
        SectionGroup {
            id: SectionId::Assistant,
            key: SectionId::Assistant.key(),
            aria_label: SectionId::Assistant.nav_aria_label(),
            entries: assistant_entries,
        },
        SectionGroup {
            id: SectionId::Work,
            key: SectionId::Work.key(),
            aria_label: SectionId::Work.nav_aria_label(),
            entries: work_entries,
        },
        SectionGroup {
            id: SectionId::Plan,
            key: SectionId::Plan.key(),
            aria_label: SectionId::Plan.nav_aria_label(),
            entries: plan_entries,
        },
        SectionGroup {
            id: SectionId::Areas,
            key: SectionId::Areas.key(),
            aria_label: SectionId::Areas.nav_aria_label(),
            entries: areas_entries,
        },
        SectionGroup {
            id: SectionId::Library,
            key: SectionId::Library.key(),
            aria_label: SectionId::Library.nav_aria_label(),
            entries: library_entries,
        },
    ];

    let tabs = SectionId::ALL
        .into_iter()
        .map(|sec| {
            let (thing, until) = sec.landing();
            let target = NavTarget::from_candidate(sec.first_path(), thing, until, planned);
            SectionTab {
                id: sec,
                key: sec.key(),
                label: sec.label(),
                target,
            }
        })
        .collect();

    Navigation {
        tabs,
        groups,
        build_status: BuildStatus::from_view(view),
    }
}
