//! DEPS-3 and the measurement report for `cargo xtask deps --check`.
//!
//! This module owns the budget half of the dependency gate: the direct shipped
//! budget and the transitive cap per profile, the comparison of the measured
//! counts from [`crate::deps_graph`] against them, and the report line that shows
//! the counts beside the budgets that apply. It starts no process and reads no
//! file; `crate::deps_policy` runs it beside the catalog checks and `crate::deps`
//! drives the command.
//!
//! The numbers are the ones in `docs/reference/dependencies.md`: the silent direct
//! ceiling is 12, or 14 when both workbook crates are active shipped normal
//! dependencies; the AI ceiling is 17; the caps are 80 and 100. The higher silent
//! ceiling needs both workbook crates as active shipped normal dependencies; a
//! catalog row, a lock entry, an `xtask` edge or a dev-only crate cannot select it.

use crate::deps_graph::{Graph, Profile};

const SILENT_DIRECT: usize = 12;
const SILENT_DIRECT_WORKBOOK: usize = 14;
const AI_DIRECT: usize = 17;
const SILENT_CAP: usize = 80;
const AI_CAP: usize = 100;

/// The direct shipped budget of one profile.
pub(crate) fn ceiling(graph: &Graph) -> usize {
    match graph.profile {
        Profile::Silent if graph.workbook() => SILENT_DIRECT_WORKBOOK,
        Profile::Silent => SILENT_DIRECT,
        Profile::Ai => AI_DIRECT,
    }
}

/// The transitive cap of one profile.
pub(crate) fn cap(profile: Profile) -> usize {
    match profile {
        Profile::Silent => SILENT_CAP,
        Profile::Ai => AI_CAP,
    }
}

/// The measured counts beside the budgets that apply to them.
pub(crate) fn summary(graph: &Graph) -> String {
    format!(
        "deps: {} direct {}/{} transitive {}/{} all-target {}",
        graph.profile.name(),
        graph.shipped_direct.len(),
        ceiling(graph),
        graph.transitive.len(),
        cap(graph.profile),
        graph.all_target.len()
    )
}

/// DEPS-3: the direct shipped budget and the transitive cap, once per profile.
pub(crate) fn deps3(silent: &Graph, ai: Option<&Graph>, findings: &mut Vec<String>) {
    budget(findings, silent);
    if let Some(ai) = ai {
        budget(findings, ai);
    }
}

fn budget(findings: &mut Vec<String>, graph: &Graph) {
    let direct = graph.shipped_direct.len();
    let ceiling = ceiling(graph);
    if direct > ceiling {
        findings.push(format!(
            "DEPS-3 {}: direct shipped crates {direct} exceed {ceiling}",
            graph.profile.name()
        ));
    }
    let transitive = graph.transitive.len();
    let cap = cap(graph.profile);
    if transitive > cap {
        findings.push(format!(
            "DEPS-3 {}: transitive crates {transitive} exceed {cap}",
            graph.profile.name()
        ));
    }
}

/// Builders for a stated measured profile, shared by the policy modules' tests.
/// The real-Cargo tests in [`crate::deps_graph`] prove the collector that
/// produces the same shape, so a policy test can state the counts it wants.
#[cfg(test)]
pub(crate) mod build {
    use crate::deps_graph::{Graph, Profile};
    use std::collections::{BTreeMap, BTreeSet};

    /// `count` external direct edges named `dep000` upward, all at `0.1.0`.
    pub(crate) fn edges(count: usize) -> Vec<(String, String)> {
        (0..count)
            .map(|index| (format!("dep{index:03}"), "0.1.0".to_owned()))
            .collect()
    }

    /// Borrowed `(name, floor)` pairs for [`graph`].
    pub(crate) fn refs(edges: &[(String, String)]) -> Vec<(&str, &str)> {
        edges
            .iter()
            .map(|(name, version)| (name.as_str(), version.as_str()))
            .collect()
    }

    /// A measured profile with `direct` normal direct edges and `transitive`
    /// counted crate/version pairs. `direct` entries are `(name, floor)`.
    pub(crate) fn graph(profile: Profile, direct: &[(&str, &str)], transitive: usize) -> Graph {
        let normal_direct = direct
            .iter()
            .map(|(name, version)| ((*name).to_owned(), BTreeSet::from([(*version).to_owned()])))
            .collect();
        Graph {
            profile,
            transitive: (0..transitive)
                .map(|index| (format!("t{index:03}"), "0.1.0".to_owned()))
                .collect(),
            shipped_direct: direct.iter().map(|(name, _)| (*name).to_owned()).collect(),
            normal_direct,
            dev_direct: BTreeMap::new(),
            build_direct: BTreeMap::new(),
            has_calamine: direct.iter().any(|(name, _)| *name == "calamine"),
            has_zip: direct.iter().any(|(name, _)| *name == "zip"),
            all_target: BTreeSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build::{edges, graph, refs};
    use super::*;
    use crate::deps_graph::{Measurements, measure};
    use std::path::{Path, PathBuf};

    fn findings(silent: &Graph, ai: Option<&Graph>) -> Vec<String> {
        let mut findings = Vec::new();
        deps3(silent, ai, &mut findings);
        findings
    }

    /// One measurement's budget findings, in the order [`deps3`] emits them.
    fn budget_findings(measured: &Measurements) -> Vec<String> {
        findings(&measured.silent, measured.ai.as_ref())
    }

    /// The immutable fixture workspaces live under `tests/fixtures/deps-graph/`.
    /// The path is bound to the crate manifest, never to the process working
    /// directory or this file's location.
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/deps-graph")
            .join(name)
    }

    fn copy(source: &Path, target: &Path) {
        std::fs::create_dir_all(target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let to = target.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &to);
            } else {
                std::fs::copy(entry.path(), to).unwrap();
            }
        }
    }

    /// Atomically claim a unique private scratch root. A guessed path is never
    /// reused or pre-deleted; the caller owns whatever it claims and leaves it.
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let candidate = std::env::temp_dir().join(format!(
                "fieldbook-deps-budget-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => return candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("scratch {}: {error}", candidate.display()),
            }
        }
    }

    /// Copy one fixture workspace tree to a fresh private root and measure its
    /// `ws/` member workspace with the real collector: a real `cargo metadata`
    /// process and the real decoder, never a constructed `Graph`.
    fn measured(name: &str) -> Measurements {
        let base = scratch(name);
        copy(&fixture(name), &base);
        measure(&base.join("ws")).unwrap()
    }

    #[test]
    fn the_report_shows_the_counts_beside_their_budgets() {
        assert_eq!(
            summary(&graph(Profile::Silent, &[("alpha", "1.0.0")], 1)),
            "deps: silent direct 1/12 transitive 1/80 all-target 0"
        );
        assert_eq!(
            summary(&graph(
                Profile::Silent,
                &[("calamine", "1.0.0"), ("zip", "1.0.0")],
                2
            )),
            "deps: silent direct 2/14 transitive 2/80 all-target 0",
            "both workbook crates select the higher silent ceiling"
        );
        assert_eq!(
            summary(&graph(Profile::Ai, &[("alpha", "1.0.0")], 1)),
            "deps: ai direct 1/17 transitive 1/100 all-target 0"
        );
    }

    /// The literal direct-budget boundaries: 12/13 before the workbook crates,
    /// 14/15 once both are active shipped normal dependencies, 17/18 for the AI
    /// profile. A single workbook crate cannot select the higher ceiling. Every
    /// assertion is the complete finding text, so a wrong ceiling, a wrong count
    /// or a missing branch fails on the literal message.
    #[test]
    fn direct_budget_boundaries() {
        let silent =
            |count: usize| findings(&graph(Profile::Silent, &refs(&edges(count)), count), None);
        assert_eq!(
            silent(12),
            Vec::<String>::new(),
            "twelve direct crates are within the silent budget"
        );
        assert_eq!(
            silent(13),
            ["DEPS-3 silent: direct shipped crates 13 exceed 12"]
        );

        let workbook = |count: usize| {
            let mut direct = edges(count);
            direct.push(("calamine".to_owned(), "0.1.0".to_owned()));
            direct.push(("zip".to_owned(), "0.1.0".to_owned()));
            findings(&graph(Profile::Silent, &refs(&direct), count + 2), None)
        };
        assert_eq!(
            workbook(12),
            Vec::<String>::new(),
            "twelve others plus both workbook crates are fourteen, within fourteen"
        );
        assert_eq!(
            workbook(13),
            ["DEPS-3 silent: direct shipped crates 15 exceed 14"]
        );

        for only in ["calamine", "zip"] {
            let mut direct = edges(12);
            direct.push((only.to_owned(), "0.1.0".to_owned()));
            assert_eq!(
                findings(&graph(Profile::Silent, &refs(&direct), 13), None),
                ["DEPS-3 silent: direct shipped crates 13 exceed 12"],
                "a single {only} must not select the higher ceiling"
            );
        }

        let silent_pass = graph(Profile::Silent, &[], 0);
        let ai = |count: usize| {
            findings(
                &silent_pass,
                Some(&graph(Profile::Ai, &refs(&edges(count)), count)),
            )
        };
        assert_eq!(
            ai(17),
            Vec::<String>::new(),
            "seventeen direct crates are within the AI budget"
        );
        assert_eq!(ai(18), ["DEPS-3 ai: direct shipped crates 18 exceed 17"]);
    }

    /// The literal transitive caps: 80/81 silent, 100/101 AI, counted as distinct
    /// crate-and-version pairs. The message carries the profile, the count and the
    /// cap, so a wrong cap or a wrong count fails on the text, not on a prefix.
    #[test]
    fn transitive_cap_boundaries() {
        assert_eq!(
            findings(&graph(Profile::Silent, &[], 80), None),
            Vec::<String>::new(),
            "a silent graph at exactly eighty passes"
        );
        assert_eq!(
            findings(&graph(Profile::Silent, &[], 81), None),
            ["DEPS-3 silent: transitive crates 81 exceed 80"]
        );

        let silent_at_cap = graph(Profile::Silent, &[], 80);
        let ai = |count: usize| findings(&silent_at_cap, Some(&graph(Profile::Ai, &[], count)));
        assert_eq!(
            ai(100),
            Vec::<String>::new(),
            "an AI graph at exactly one hundred passes"
        );
        assert_eq!(ai(101), ["DEPS-3 ai: transitive crates 101 exceed 100"]);
    }

    /// The count is of distinct crate-and-version pairs, not of crate names:
    /// eighty versions of one crate are eighty pairs, and the eighty-first fails.
    #[test]
    fn the_transitive_cap_counts_distinct_name_version_pairs() {
        let mut many = graph(Profile::Silent, &[], 0);
        for minor in 0..80 {
            many.transitive
                .insert(("repeated".to_owned(), format!("0.{minor}.0")));
        }
        assert_eq!(
            summary(&many),
            "deps: silent direct 0/12 transitive 80/80 all-target 0"
        );
        assert_eq!(
            findings(&many, None),
            Vec::<String>::new(),
            "eighty pairs of one name are eighty transitive crates"
        );
        many.transitive
            .insert(("repeated".to_owned(), "0.80.0".to_owned()));
        assert_eq!(
            findings(&many, None),
            ["DEPS-3 silent: transitive crates 81 exceed 80"]
        );
    }

    /// Every exceeded budget contributes exactly one finding, direct before
    /// transitive and silent before ai, and the report still shows the budget that
    /// was exceeded beside the count.
    #[test]
    fn every_exceeded_budget_contributes_one_named_finding() {
        let silent = graph(Profile::Silent, &refs(&edges(13)), 81);
        let ai = graph(Profile::Ai, &refs(&edges(18)), 101);
        assert_eq!(
            summary(&silent),
            "deps: silent direct 13/12 transitive 81/80 all-target 0"
        );
        assert_eq!(
            findings(&silent, Some(&ai)),
            [
                "DEPS-3 silent: direct shipped crates 13 exceed 12",
                "DEPS-3 silent: transitive crates 81 exceed 80",
                "DEPS-3 ai: direct shipped crates 18 exceed 17",
                "DEPS-3 ai: transitive crates 101 exceed 100",
            ]
        );
        assert_eq!(
            findings(&silent, None),
            [
                "DEPS-3 silent: direct shipped crates 13 exceed 12",
                "DEPS-3 silent: transitive crates 81 exceed 80",
            ],
            "an inapplicable AI profile contributes no AI finding"
        );
    }

    /// The all-target closure is reported beside the capped count and is never
    /// capped, in either profile. The first pair is a measured fixture; the second
    /// states what the report must say when the all-target closure is larger than
    /// the union of the two graph targets.
    #[test]
    fn the_all_target_closure_is_reported_and_not_capped() {
        let real = measured("graph");
        assert_eq!(
            summary(&real.silent),
            "deps: silent direct 3/12 transitive 4/80 all-target 4"
        );
        let ai = real
            .ai
            .as_ref()
            .expect("the fixture's `net` feature is applicable");
        assert_eq!(
            summary(ai),
            "deps: ai direct 4/17 transitive 5/100 all-target 5"
        );

        let mut silent = graph(Profile::Silent, &refs(&edges(12)), 80);
        silent.all_target = (0..95)
            .map(|index| (format!("a{index:03}"), "0.1.0".to_owned()))
            .collect();
        assert_eq!(
            summary(&silent),
            "deps: silent direct 12/12 transitive 80/80 all-target 95"
        );
        assert_eq!(
            findings(&silent, None),
            Vec::<String>::new(),
            "an all-target count above every cap is reported, never capped"
        );

        let mut ai = graph(Profile::Ai, &refs(&edges(17)), 100);
        ai.all_target = (0..106)
            .map(|index| (format!("a{index:03}"), "0.1.0".to_owned()))
            .collect();
        assert_eq!(
            summary(&ai),
            "deps: ai direct 17/17 transitive 100/100 all-target 106"
        );
        assert_eq!(
            findings(&graph(Profile::Silent, &[], 0), Some(&ai)),
            Vec::<String>::new(),
            "the AI all-target count is uncapped too"
        );
    }

    /// The budget is applied to what the real collector measured, not only to a
    /// constructed `Graph`: each case copies a fixture workspace to a private root,
    /// runs the real `cargo metadata` measurement, and asserts the literal report
    /// and findings. `workbook` holds both workbook crates as active shipped normal
    /// dependencies, `workbook-dev` holds `zip` as a dev dependency, and
    /// `shipped-namesake` holds an external package named `fieldbook` beside
    /// `calamine`, whose own `zip` dependency is transitive.
    #[test]
    fn the_measured_workbook_edges_select_the_silent_ceiling() {
        let workbook = measured("workbook");
        assert_eq!(
            workbook
                .silent
                .shipped_direct
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            ["calamine", "zip"],
            "both workbook crates are active shipped normal dependencies"
        );
        assert_eq!(ceiling(&workbook.silent), 14);
        assert_eq!(
            summary(&workbook.silent),
            "deps: silent direct 2/14 transitive 2/80 all-target 2"
        );
        assert_eq!(budget_findings(&workbook), Vec::<String>::new());

        let dev_only = measured("workbook-dev");
        assert_eq!(
            ceiling(&dev_only.silent),
            12,
            "a dev-only `zip` must not select the higher silent ceiling"
        );
        assert_eq!(
            summary(&dev_only.silent),
            "deps: silent direct 1/12 transitive 2/80 all-target 2"
        );
        assert_eq!(budget_findings(&dev_only), Vec::<String>::new());

        let namesake = measured("shipped-namesake");
        assert_eq!(
            namesake
                .silent
                .shipped_direct
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            ["calamine", "fieldbook"],
            "the namesake is a shipped direct crate, the transitive `zip` is not"
        );
        assert!(
            namesake
                .silent
                .transitive
                .contains(&("zip".to_owned(), "0.1.0".to_owned())),
            "the fixture's `zip` is in the graph, brought by `calamine`"
        );
        assert_eq!(
            ceiling(&namesake.silent),
            12,
            "a transitive or namesake `zip` cannot select the higher ceiling"
        );
        assert_eq!(
            summary(&namesake.silent),
            "deps: silent direct 2/12 transitive 3/80 all-target 3"
        );
    }

    /// `xtask` counts toward the transitive cap and never toward the shipped direct
    /// budget, on a real measured fixture; each profile is budgeted once, and an
    /// absent AI profile contributes no finding.
    #[test]
    fn the_measured_profiles_are_budgeted_separately() {
        let fixture = measured("graph");
        assert_eq!(
            fixture
                .silent
                .shipped_direct
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            ["both", "normal", "winlib"],
            "`xtask`'s crate is never a direct shipped crate"
        );
        assert!(
            fixture
                .silent
                .transitive
                .contains(&("xtool".to_owned(), "0.1.0".to_owned())),
            "`xtask` counts toward the transitive cap"
        );
        let ai = fixture
            .ai
            .as_ref()
            .expect("the fixture's `net` feature is applicable");
        assert_eq!(ceiling(&fixture.silent), 12);
        assert_eq!(ceiling(ai), 17);
        assert_eq!(cap(Profile::Silent), 80);
        assert_eq!(cap(ai.profile), 100);
        assert_eq!(budget_findings(&fixture), Vec::<String>::new());

        let inapplicable = measured("no-net");
        assert!(inapplicable.ai.is_none());
        assert_eq!(
            summary(&inapplicable.silent),
            "deps: silent direct 1/12 transitive 1/80 all-target 1"
        );
        assert_eq!(
            budget_findings(&inapplicable),
            Vec::<String>::new(),
            "an absent AI profile contributes no budget finding"
        );
    }
}
