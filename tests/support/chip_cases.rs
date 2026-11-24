#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChipContext {
    Home,
    Laptop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipOutcome {
    Exit0,
    FindingsExit1(&'static str),
    RefusalExit2(&'static str),
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChipCase {
    pub chip: &'static str,
    pub context: ChipContext,
    pub samples: &'static [(&'static str, &'static str)],
    pub fixture: &'static str,
    pub setup: &'static [&'static str],
    pub outcome: ChipOutcome,
}

pub const DEFAULT_SAMPLES: &[(&str, &str)] = &[
    ("<area>", "fnd"),
    ("<slug>", "sample"),
    ("<type>", "concept"),
    ("<subarea>", "overview"),
    ("<a>", "fnd"),
    ("<text>", "Sample text?"),
    ("<version-id>", "S-0001@1"),
    ("<locator>", "p.1"),
    ("<file>", "sample.txt"),
    ("<source-id>", "S-0001"),
    ("<query>", "fnd"),
    ("<claim-id>", "C-0001"),
    ("YYYY-MM-DD", "2026-03-01"),
    ("<folder>", "incoming"),
    ("<n>", "1"),
    ("<question>", "What is the policy?"),
    ("<proposal-id>", "P-0001"),
    ("<word>", "duplicate"),
];

pub const CASES: &[ChipCase] = &[
    ChipCase {
        chip: "fieldbook check",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook fmt",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook search <query>",
        context: ChipContext::Home,
        samples: &[("<query>", "CL-CMP-0001")],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook audit verify",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/state-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook audit show",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/state-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook new page --confirm-public <area>-<slug> --type <type> --subarea <subarea>",
        context: ChipContext::Home,
        samples: &[
            ("<area>", "cmp"),
            ("<slug>", "synthetic-test"),
            ("<subarea>", "business-systems"),
        ],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook new question --confirm-public --area <a> \"<text>\"",
        context: ChipContext::Home,
        samples: &[("<a>", "cmp")],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook new claim --confirm-public --area <a> --source <version-id> --at \"<locator>\" --quote \"<text>\"",
        context: ChipContext::Home,
        samples: &[
            ("<a>", "cmp"),
            ("<version-id>", "S-0002-v2"),
            ("<locator>", "p. 1"),
        ],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook new conflict --confirm-public --area <a>",
        context: ChipContext::Home,
        samples: &[("<a>", "cmp")],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook new term --confirm-public --area <a>",
        context: ChipContext::Home,
        samples: &[("<a>", "cmp")],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook add <file>",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &["sample.txt"],
        outcome: ChipOutcome::FindingsExit1("FB0410"),
    },
    ChipCase {
        chip: "fieldbook add <file> --source <source-id>",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &["sample.txt"],
        outcome: ChipOutcome::FindingsExit1("FB0410"),
    },
    ChipCase {
        chip: "fieldbook add <folder>",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &["incoming"],
        outcome: ChipOutcome::FindingsExit1("FB0410"),
    },
    ChipCase {
        chip: "fieldbook verify <claim-id> --on YYYY-MM-DD",
        context: ChipContext::Home,
        samples: &[("<claim-id>", "CL-CMP-0002")],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Terminal,
    },
    ChipCase {
        chip: "fieldbook status",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook status --page",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook build",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook backup",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook backup verify",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook backup list",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook restore --snapshot <n> --drill",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
    ChipCase {
        chip: "fieldbook policy show",
        context: ChipContext::Home,
        samples: &[],
        fixture: "tests/fixtures/corpus-mini",
        setup: &[],
        outcome: ChipOutcome::Exit0,
    },
];

pub fn substitute_chip(chip: &str, samples: &[(&str, &str)]) -> Vec<String> {
    let mut text = chip.to_owned();
    for &(key, val) in samples {
        if !key.is_empty() && text.contains(key) {
            text = text.replace(key, val);
        } else {
            text.push(' ');
            text.push_str(val);
        }
    }
    for &(key, val) in DEFAULT_SAMPLES {
        if text.contains(key) {
            text = text.replace(key, val);
        }
    }
    split_argv(&text)
}

pub fn split_argv(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in cmd.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

pub fn terminal_only(cases: &[ChipCase]) -> Vec<&'static str> {
    cases
        .iter()
        .filter(|c| matches!(c.outcome, ChipOutcome::Terminal))
        .map(|c| c.chip)
        .collect()
}
