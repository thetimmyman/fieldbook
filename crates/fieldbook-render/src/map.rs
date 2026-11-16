//! `map.html`, the App map: five layer bands, the Assistant column as text, and the Releases
//! and Labels and stores panels (`app-map`).

use askama::Template;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::{PageType, Register};
use fieldbook_core::strings;

use crate::about::{self, NETWORK_POSTURE_SHORT};
use crate::area;
use crate::day_one;
use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::home;
use crate::ids;
use crate::links::{escape_html, href_if_planned};
use crate::meta::{BuildProfile, RenderContext};
use crate::nav::{self, Navigation, SectionId};
use crate::register;
use crate::search_page;
use crate::site::PlannedPaths;
use crate::source::render_unavailable;

/// Site-relative output path of `map.html`.
pub const OUTPUT_PATH: &str = strings::paths()[5];

/// Document and breadcrumb title of `map.html`.
pub const PAGE_TITLE: &str = "App map";

const HEADING: &str = "How Fieldbook fits together";
const LEDE: &str = "The library is the foundation, not the point. The point is the decision: every figure in a package traces to its assumption, source, owner and version.";

const OUTPUTS_NOTE: &str = "Outputs that use company data are made from R2, in company storage, and leave it only where policy allows. Planned pricing modules use the same evidence and approval recipes.";

const ASSISTANT_HEADING: &str = "Assistant";
const ASSISTANT_LEAD: &str = "A chat in every layer. Fieldbook picks the context from the page you are on and shows it. You can pin, remove or add items.";

const ADAPTER_ROWS: [(&str, &str); 3] = [
    (
        "Handoff",
        "Copy the context pack, paste the answer back, and its citations are checked.",
    ),
    ("Off", "No model connected here"),
    ("Connected", "Where a model adapter exists."),
];

const RELEASES: [(&str, &str); 4] = [
    ("R1", "Day-one Corpus: edition-001 and tool v0.1.0."),
    (
        "E1",
        "The first edition, moved to the laptop over the approved channel.",
    ),
    (
        "R2",
        "Signed Toolkit: v1.0.0. A signed command-line tool, installed by IT.",
    ),
    ("R3", "Modules FM0 to FM14, each gated on a named question."),
];

const LABELS_LEAD: &str =
    "Every page, source and record carries labels. A record goes only where its labels allow.";

const LABELS: [(&str, &str); 4] = [
    ("public", "A public source or a synthetic example."),
    (
        "unlabelled",
        "No label could be found. It stays local and goes out only in a company-storage backup.",
    ),
    (
        "marked",
        "A marking was found and no rule maps it. It is held locally and never leaves.",
    ),
    (
        "owner-supplied",
        "Text you typed or pasted for one request. Only request logs hold it.",
    ),
];

const STORES: [(&str, &[&str]); 4] = [
    ("corpus", &["public"]),
    ("overlay", &["public", "unlabelled", "marked"]),
    ("working", &["public", "unlabelled", "marked"]),
    (
        "state",
        &["public", "unlabelled", "owner-supplied", "marked"],
    ),
];

const MKT_VIEW_CODES: [&str; 3] = ["CPY", "CPT", "PGM"];

struct PillSpec {
    code: Option<String>,
    label: String,
    target: String,
    thing: String,
    until: &'static str,
}

struct PillView {
    code: Option<String>,
    label: String,
    href: String,
}

struct UnbuiltView {
    code: Option<String>,
    text: String,
}

struct BandView {
    section_class: &'static str,
    title: &'static str,
    subtitle: &'static str,
    pills: Vec<PillView>,
    unbuilt: Vec<UnbuiltView>,
    unavailable_html: Option<String>,
    note: Option<&'static str>,
}

struct AdapterRowView {
    label: &'static str,
    line: &'static str,
}

struct ReleaseRowView {
    name: &'static str,
    line: &'static str,
}

struct LabelRowView {
    label: &'static str,
    line: &'static str,
}

struct StoreRowView {
    name: &'static str,
    holds: &'static [&'static str],
}

#[derive(Template)]
#[template(path = "./map.html")]
struct MapTemplate {
    heading: &'static str,
    lede: &'static str,
    bands: Vec<BandView>,
    assistant_heading: &'static str,
    assistant_lead: &'static str,
    adapter_rows: Vec<AdapterRowView>,
    posture_html: Option<String>,
    releases: Vec<ReleaseRowView>,
    about_href: Option<String>,
    labels_lead: &'static str,
    labels: Vec<LabelRowView>,
    stores: Vec<StoreRowView>,
}

/// The posture sentence as HTML: its Markdown code spans become `<code>`, so the constant can
/// quote the canonical sentence exactly and the reader sees no backticks.
fn posture_html(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 32);
    for (i, part) in line.split('`').enumerate() {
        if i % 2 == 1 {
            out.push_str("<code>");
            out.push_str(&escape_html(part));
            out.push_str("</code>");
        } else {
            out.push_str(&escape_html(part));
        }
    }
    out
}

const fn register_pill(reg: Register) -> (&'static str, &'static str) {
    match reg {
        Register::Claims => ("Claims", "The claims register"),
        Register::Questions => ("Questions", "The questions register"),
        Register::Conflicts => ("Conflicts", "The conflicts register"),
        Register::Glossary => ("Glossary", "The glossary"),
        Register::Sources => (strings::parts()[6], "The sources register"),
    }
}

fn partition_pills(
    specs: Vec<PillSpec>,
    planned: &PlannedPaths,
) -> (Vec<PillView>, Vec<UnbuiltView>) {
    let mut pills = Vec::new();
    let mut unbuilt = Vec::new();
    for spec in specs {
        if let Some(href) = href_if_planned(OUTPUT_PATH, &spec.target, planned) {
            pills.push(PillView {
                code: spec.code,
                label: spec.label,
                href,
            });
        } else {
            unbuilt.push(UnbuiltView {
                code: spec.code,
                text: nav::format_not_available(&spec.thing, spec.until),
            });
        }
    }
    (pills, unbuilt)
}

fn build_outputs_band(view: &LabelledView, planned: &PlannedPaths) -> BandView {
    let mut specs = vec![PillSpec {
        code: None,
        label: day_one::PAGE_TITLE.to_owned(),
        target: day_one::OUTPUT_PATH.to_owned(),
        thing: "The day-one binder".to_owned(),
        until: "Day 1",
    }];
    for page in view.pages() {
        if page.meta.r#type == PageType::Brief {
            specs.push(PillSpec {
                code: Some(page.meta.area.as_str().to_ascii_uppercase()),
                label: page.meta.title.clone(),
                target: format!("p/{}.html", page.id),
                thing: page.meta.title.clone(),
                until: "Day 1",
            });
        }
    }
    specs.extend([
        PillSpec {
            code: None,
            label: "Changes".to_owned(),
            target: "changes.html".to_owned(),
            thing: "Changes".to_owned(),
            until: "edition-002",
        },
        PillSpec {
            code: Some("ORG".to_owned()),
            label: "Decision packages".to_owned(),
            target: "decisions.html".to_owned(),
            thing: "Decision packages".to_owned(),
            until: "Release 2",
        },
        PillSpec {
            code: Some("ORG".to_owned()),
            label: "Monthly project reviews".to_owned(),
            target: "projects.html".to_owned(),
            thing: "Monthly project reviews".to_owned(),
            until: "the ORG overlay, Release 2 or later",
        },
        PillSpec {
            code: Some("FM7".to_owned()),
            label: "Leadership reports".to_owned(),
            target: "reports.html".to_owned(),
            thing: "Leadership reports".to_owned(),
            until: "Release 3",
        },
    ]);
    let (pills, unbuilt) = partition_pills(specs, planned);
    BandView {
        section_class: "layer",
        title: "Outputs",
        subtitle: "What you hand on",
        pills,
        unbuilt,
        unavailable_html: None,
        note: Some(OUTPUTS_NOTE),
    }
}

fn build_work_core_band(view: &LabelledView, planned: &PlannedPaths) -> BandView {
    let mut specs = vec![PillSpec {
        code: None,
        label: "Today".to_owned(),
        target: home::OUTPUT_PATH.to_owned(),
        thing: "Today".to_owned(),
        until: "Day 1",
    }];
    for reg in Register::ALL {
        let (label, thing) = register_pill(reg);
        specs.push(PillSpec {
            code: None,
            label: label.to_owned(),
            target: register::output_path(reg).to_owned(),
            thing: thing.to_owned(),
            until: "Day 1",
        });
    }
    for page in view.pages() {
        if page.meta.stage == 1 || page.meta.r#type == PageType::Checklist {
            specs.push(PillSpec {
                code: Some(page.meta.area.as_str().to_ascii_uppercase()),
                label: page.meta.title.clone(),
                target: format!("p/{}.html", page.id),
                thing: page.meta.title.clone(),
                until: "Day 1",
            });
        }
    }
    specs.extend([
        PillSpec {
            code: None,
            label: "Bids & proposals".to_owned(),
            target: "bids.html".to_owned(),
            thing: "Bids".to_owned(),
            until: "the ORG overlay, Release 2 or later",
        },
        PillSpec {
            code: None,
            label: "Estimates".to_owned(),
            target: "estimates.html".to_owned(),
            thing: "Estimates".to_owned(),
            until: "Release 2",
        },
        PillSpec {
            code: None,
            label: "Decisions".to_owned(),
            target: "decisions.html".to_owned(),
            thing: "Decisions".to_owned(),
            until: "Release 2",
        },
        PillSpec {
            code: None,
            label: "Projects".to_owned(),
            target: "projects.html".to_owned(),
            thing: "Projects".to_owned(),
            until: "the ORG overlay, Release 2 or later",
        },
        PillSpec {
            code: None,
            label: "Assumptions".to_owned(),
            target: "assumptions.html".to_owned(),
            thing: "Assumptions".to_owned(),
            until: "Release 2",
        },
        PillSpec {
            code: None,
            label: "Discovery".to_owned(),
            target: "discovery.html".to_owned(),
            thing: "Discovery".to_owned(),
            until: "Release 2",
        },
        PillSpec {
            code: None,
            label: "Audit & requests".to_owned(),
            target: "audit.html".to_owned(),
            thing: "Audit requests".to_owned(),
            until: "Release 2",
        },
    ]);
    let (pills, unbuilt) = partition_pills(specs, planned);
    BandView {
        section_class: "layer core",
        title: "Work core",
        subtitle: "Bids, estimates, decisions and projects on one assumption register, inside the 90-day plan",
        pills,
        unbuilt,
        unavailable_html: None,
        note: None,
    }
}

fn build_workspaces_band(view: &LabelledView, planned: &PlannedPaths) -> BandView {
    let mut specs = Vec::new();
    if let Some(corpus) = view.corpus() {
        for def in &corpus.area {
            specs.push(PillSpec {
                code: Some(def.code.to_ascii_uppercase()),
                label: def.name.clone(),
                target: area::page_path(&def.code),
                thing: def.name.clone(),
                until: "Day 1",
            });
        }
        for wanted in MKT_VIEW_CODES {
            if let Some(v) = corpus
                .view
                .iter()
                .find(|v| v.code.eq_ignore_ascii_case(wanted))
            {
                let path = area::page_path(&v.code);
                if planned.contains(&path) {
                    specs.push(PillSpec {
                        code: Some(v.code.to_ascii_uppercase()),
                        label: v.name.clone(),
                        target: path,
                        thing: v.name.clone(),
                        until: "Day 30",
                    });
                }
            }
        }
    }
    let (pills, unbuilt) = partition_pills(specs, planned);
    BandView {
        section_class: "layer",
        title: "Workspaces",
        subtitle: "Processes, procedures, templates, module and history per area",
        pills,
        unbuilt,
        unavailable_html: None,
        note: None,
    }
}

fn build_modules_band() -> BandView {
    let unavailable = render_unavailable("Modules", "R3").replace(
        "Modules is not available until R3.",
        "Modules are not available until R3.",
    );
    BandView {
        section_class: "layer",
        title: "Modules",
        subtitle: "FM0 to FM14, each gated on a named question",
        pills: Vec::new(),
        unbuilt: Vec::new(),
        unavailable_html: Some(unavailable),
        note: None,
    }
}

fn build_library_band(view: &LabelledView, planned: &PlannedPaths) -> BandView {
    let mut specs = vec![
        PillSpec {
            code: None,
            label: "All IDs".to_owned(),
            target: ids::OUTPUT_PATH.to_owned(),
            thing: "All IDs".to_owned(),
            until: "Day 1",
        },
        PillSpec {
            code: None,
            label: "Search".to_owned(),
            target: search_page::OUTPUT_PATH.to_owned(),
            thing: "Search".to_owned(),
            until: "Day 1",
        },
    ];
    for reg in Register::ALL {
        let (label, thing) = register_pill(reg);
        specs.push(PillSpec {
            code: None,
            label: label.to_owned(),
            target: register::output_path(reg).to_owned(),
            thing: thing.to_owned(),
            until: "Day 1",
        });
    }
    for page in view.pages() {
        if page.meta.r#type == PageType::Reference {
            specs.push(PillSpec {
                code: Some(page.meta.area.as_str().to_ascii_uppercase()),
                label: page.meta.title.clone(),
                target: format!("p/{}.html", page.id),
                thing: page.meta.title.clone(),
                until: "Day 1",
            });
        }
    }
    specs.push(PillSpec {
        code: None,
        label: "Feeds".to_owned(),
        target: "feeds.html".to_owned(),
        thing: "Feeds".to_owned(),
        until: "the first admitted feed",
    });
    let (pills, unbuilt) = partition_pills(specs, planned);
    BandView {
        section_class: "layer found",
        title: "Library",
        subtitle: "Foundation layer: cited rules, market evidence, definitions",
        pills,
        unbuilt,
        unavailable_html: None,
        note: None,
    }
}

/// Renders the `<main>` body of `map.html`.
#[must_use]
pub fn render_main(
    view: &LabelledView,
    ctx: &RenderContext,
    profile: BuildProfile,
    planned: &PlannedPaths,
) -> String {
    let _ = ctx;
    let bands = vec![
        build_outputs_band(view, planned),
        build_work_core_band(view, planned),
        build_workspaces_band(view, planned),
        build_modules_band(),
        build_library_band(view, planned),
    ];
    let adapter_rows = ADAPTER_ROWS
        .into_iter()
        .map(|(label, line)| AdapterRowView { label, line })
        .collect();
    let posture = match profile {
        BuildProfile::Silent => Some(posture_html(NETWORK_POSTURE_SHORT)),
        BuildProfile::Ai => None,
    };
    let releases = RELEASES
        .into_iter()
        .map(|(name, line)| ReleaseRowView { name, line })
        .collect();
    let about_href = href_if_planned(OUTPUT_PATH, about::OUTPUT_PATH, planned);
    let labels = LABELS
        .into_iter()
        .map(|(label, line)| LabelRowView { label, line })
        .collect();
    let stores = STORES
        .into_iter()
        .map(|(name, holds)| StoreRowView { name, holds })
        .collect();

    MapTemplate {
        heading: HEADING,
        lede: LEDE,
        bands,
        assistant_heading: ASSISTANT_HEADING,
        assistant_lead: ASSISTANT_LEAD,
        adapter_rows,
        posture_html: posture,
        releases,
        about_href,
        labels_lead: LABELS_LEAD,
        labels,
        stores,
    }
    .render()
    .unwrap_or_else(|e| unreachable!("map.html is infallible: {e}"))
}

/// Renders the complete `map.html` document inside the global frame.
#[must_use]
pub fn render_page(
    view: &LabelledView,
    ctx: &RenderContext,
    profile: BuildProfile,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_main(view, ctx, profile, planned);
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let today_crumb = match SectionId::Today.first_path() {
        Some(first) => Crumb::linked("Today", first, planned),
        None => Crumb::current("Today"),
    };
    let crumbs = [today_crumb, Crumb::current(PAGE_TITLE)];
    render_frame_page(&FrameInput {
        output_path: OUTPUT_PATH,
        title: PAGE_TITLE,
        page_area: None,
        edition: ctx.edition_info(),
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}
