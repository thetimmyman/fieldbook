//! Scale fixture generation: pure deterministic synthetic Corpus generator.
//!
//! Generates a valid synthetic Corpus conforming to all strict check passes,
//! scalable by multiplier `m`.

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "rng module has helper methods not all consumed by scale fixture generator in bin target"
    )
)]
#[cfg_attr(
    test,
    expect(
        clippy::duplicate_mod,
        reason = "shared seeded test RNG is included at file level per card spec"
    )
)]
#[path = "../../tests/support/rng.rs"]
mod rng;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use fieldbook_core::schema::Register;
use fieldbook_core::schema::table::normalize;
use fieldbook_core::schema::vocab::PageType;
use serde::Deserialize;
use sha2::Digest;

const TODAY_TEXT: &str = include_str!("../../tests/fixtures/corpus-mini/TODAY");

/// Base Day-1 counts from `scale/base.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Base {
    /// Random seed for pseudo-word and text generation.
    pub seed: u64,
    /// Number of subject areas to generate.
    pub areas: u64,
    /// Base number of pages (multiplied by `m`).
    pub pages: u64,
    /// Ceiling for filler words added to each page section.
    pub words_per_section: u64,
    /// Base number of source rows (multiplied by `m`).
    pub source_versions: u64,
    /// Base number of stored original files (multiplied by `m`).
    pub stored_originals: u64,
    /// Byte length of each stored original file.
    pub original_bytes: u64,
    /// Base number of claims (multiplied by `m`).
    pub claims: u64,
    /// Base number of questions (multiplied by `m`).
    pub questions: u64,
    /// Base number of conflicts (multiplied by `m`).
    pub conflicts: u64,
    /// Base number of glossary terms (multiplied by `m`).
    pub glossary_terms: u64,
}

impl Base {
    /// Parse and validate base counts from TOML text.
    ///
    /// Every error message includes the offending key name.
    pub fn parse(s: &str) -> Result<Self, String> {
        let table: toml::Table = toml::from_str(s).map_err(|err| err.to_string())?;
        for (key, val) in &table {
            if !val.is_integer() {
                return Err(format!("field `{key}` must be an integer"));
            }
            if val.as_integer().is_some_and(|n| n <= 0) {
                return Err(format!("field `{key}` must be greater than zero"));
            }
        }
        let base: Self = toml::from_str(s).map_err(|err| err.to_string())?;
        Self::check_non_zero(&base)?;
        Ok(base)
    }

    fn check_non_zero(base: &Self) -> Result<(), String> {
        if base.seed == 0 {
            return Err("`seed` must be greater than zero".to_owned());
        }
        if base.areas == 0 {
            return Err("`areas` must be greater than zero".to_owned());
        }
        if base.pages == 0 {
            return Err("`pages` must be greater than zero".to_owned());
        }
        if base.words_per_section == 0 {
            return Err("`words_per_section` must be greater than zero".to_owned());
        }
        if base.source_versions == 0 {
            return Err("`source_versions` must be greater than zero".to_owned());
        }
        if base.stored_originals == 0 {
            return Err("`stored_originals` must be greater than zero".to_owned());
        }
        if base.original_bytes == 0 {
            return Err("`original_bytes` must be greater than zero".to_owned());
        }
        if base.claims == 0 {
            return Err("`claims` must be greater than zero".to_owned());
        }
        if base.questions == 0 {
            return Err("`questions` must be greater than zero".to_owned());
        }
        if base.conflicts == 0 {
            return Err("`conflicts` must be greater than zero".to_owned());
        }
        if base.glossary_terms == 0 {
            return Err("`glossary_terms` must be greater than zero".to_owned());
        }
        Ok(())
    }
}

/// Scaled counts computed from `Base` and multiplier `m`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sizes {
    /// Multiplier used.
    pub m: u64,
    /// Seed (unscaled).
    pub seed: u64,
    /// Areas (unscaled).
    pub areas: u64,
    /// Scaled pages.
    pub pages: u64,
    /// Words per section (unscaled).
    pub words_per_section: u64,
    /// Scaled source versions.
    pub source_versions: u64,
    /// Scaled stored originals.
    pub stored_originals: u64,
    /// Original bytes (unscaled).
    pub original_bytes: u64,
    /// Scaled claims.
    pub claims: u64,
    /// Scaled questions.
    pub questions: u64,
    /// Scaled conflicts.
    pub conflicts: u64,
    /// Scaled glossary terms.
    pub glossary_terms: u64,
}

/// Helper to compute source ID count given source version count where every 3rd source has 2 versions.
fn count_source_ids(source_versions: u64) -> u64 {
    let full_cycles = source_versions / 4;
    let rem = source_versions % 4;
    let rem_sources = match rem {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => 3,
    };
    full_cycles.saturating_mul(3).saturating_add(rem_sources)
}

/// Validate configuration and capacity before generating files.
pub fn validate(base: &Base, m: u64) -> Result<Sizes, String> {
    if m == 0 {
        return Err("multiplier `m` must be at least 1".to_owned());
    }

    let pages = base
        .pages
        .checked_mul(m)
        .ok_or_else(|| "multiplication overflow for `pages`".to_owned())?;
    let source_versions = base
        .source_versions
        .checked_mul(m)
        .ok_or_else(|| "multiplication overflow for `source_versions`".to_owned())?;
    let stored_originals = base
        .stored_originals
        .checked_mul(m)
        .ok_or_else(|| "multiplication overflow for `stored_originals`".to_owned())?;
    let claims = base
        .claims
        .checked_mul(m)
        .ok_or_else(|| "multiplication overflow for `claims`".to_owned())?;
    let questions = base
        .questions
        .checked_mul(m)
        .ok_or_else(|| "multiplication overflow for `questions`".to_owned())?;
    let conflicts = base
        .conflicts
        .checked_mul(m)
        .ok_or_else(|| "multiplication overflow for `conflicts`".to_owned())?;
    let glossary_terms = base
        .glossary_terms
        .checked_mul(m)
        .ok_or_else(|| "multiplication overflow for `glossary_terms`".to_owned())?;

    if questions > 999 {
        return Err(format!(
            "`questions` count {questions} exceeds 3-digit capacity limit of 999 (Q-001 to Q-999)"
        ));
    }

    let num_source_ids = count_source_ids(source_versions);
    if num_source_ids > 9999 {
        return Err(format!(
            "`source_versions` count {source_versions} yields {num_source_ids} source IDs, exceeding 4-digit limit of 9,999 (S-0001 to S-9999)"
        ));
    }

    if base.areas > 25 * 26 * 26 {
        return Err(format!(
            "`areas` count {} exceeds capacity limit of 16,900 (25*26*26 lowercase codes not starting with 'x')",
            base.areas
        ));
    }

    let max_claims_per_area = claims
        .checked_add(base.areas.saturating_sub(1))
        .map_or(u64::MAX, |sum| sum / base.areas);
    if max_claims_per_area > 9999 {
        return Err(format!(
            "`claims` per area count {max_claims_per_area} exceeds 4-digit limit of 9,999 (CL-AAA-0001 to CL-AAA-9999)"
        ));
    }

    if stored_originals > source_versions {
        return Err(format!(
            "`stored_originals` {stored_originals} must be <= `source_versions` {source_versions}"
        ));
    }

    if stored_originals < 1 {
        return Err("`stored_originals` must be at least 1".to_owned());
    }

    if base.original_bytes > 50_000_000 {
        return Err(format!(
            "`original_bytes` {} exceeds sanity limit of 50,000,000",
            base.original_bytes
        ));
    }

    if base.words_per_section > 10_000 {
        return Err(format!(
            "`words_per_section` {} exceeds sanity limit of 10,000",
            base.words_per_section
        ));
    }

    if conflicts > questions {
        return Err(format!(
            "`conflicts` {conflicts} exceeds available `questions` {questions}"
        ));
    }

    if conflicts.saturating_mul(2) > claims {
        return Err(format!(
            "`conflicts` {conflicts} requires at least {} `claims`, but found {claims}",
            conflicts * 2
        ));
    }

    if pages > 1_000_000 {
        return Err(format!("`pages` {pages} exceeds sanity limit of 1,000,000"));
    }

    if glossary_terms > 1_000_000 {
        return Err(format!(
            "`glossary_terms` {glossary_terms} exceeds sanity limit of 1,000,000"
        ));
    }

    Ok(Sizes {
        m,
        seed: base.seed,
        areas: base.areas,
        pages,
        words_per_section: base.words_per_section,
        source_versions,
        stored_originals,
        original_bytes: base.original_bytes,
        claims,
        questions,
        conflicts,
        glossary_terms,
    })
}

/// Statistics and record counts for a generated synthetic Corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Number of subject areas.
    pub areas: u64,
    /// Number of pages.
    pub pages: u64,
    /// Actual number of level-2 sections with text across all pages.
    pub sections: u64,
    /// Number of claims.
    pub claims: u64,
    /// Number of source rows.
    pub sources: u64,
    /// Number of distinct source IDs.
    pub source_ids: u64,
    /// Number of stored original files.
    pub stored_originals: u64,
    /// Number of questions.
    pub questions: u64,
    /// Number of conflicts.
    pub conflicts: u64,
    /// Number of glossary terms.
    pub terms: u64,
    /// Total number of generated files.
    pub files: u64,
    /// Total bytes across all generated files.
    pub corpus_bytes: u64,
}

/// Output of synthetic Corpus generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    /// Files as `(path, bytes)` pairs in path (byte) order.
    pub files: Vec<(String, Vec<u8>)>,
    /// Summary counts of generated items.
    pub counts: Counts,
    /// The most common generated pseudo-word (search query).
    pub query: String,
}

/// Area definition generated for `corpus/corpus.toml`.
struct GeneratedArea {
    code: String,
    slug: String,
    subareas: Vec<String>,
}

fn build_areas(count: u64) -> Result<Vec<GeneratedArea>, String> {
    let mut areas = Vec::with_capacity(usize::try_from(count).map_err(|e| e.to_string())?);
    for i in 0..count {
        let first_idx = i / (26 * 26);
        let rem = i % (26 * 26);
        let second_idx = rem / 26;
        let third_idx = rem % 26;

        let first_char = if first_idx < 23 {
            let offset = u8::try_from(first_idx).map_err(|e| e.to_string())?;
            (b'a' + offset) as char
        } else {
            let offset = u8::try_from(first_idx - 23).map_err(|e| e.to_string())?;
            (b'y' + offset) as char
        };
        let second_char = (b'a' + u8::try_from(second_idx).map_err(|e| e.to_string())?) as char;
        let third_char = (b'a' + u8::try_from(third_idx).map_err(|e| e.to_string())?) as char;

        let code = format!("{first_char}{second_char}{third_char}");
        let slug = format!("area-{code}");
        areas.push(GeneratedArea {
            code,
            slug,
            subareas: vec!["core".to_owned(), "topics".to_owned()],
        });
    }
    Ok(areas)
}

const CONSONANTS: &[u8] = b"bcdfghjklmnprstvwz";
const VOWELS: &[u8] = b"aeiou";

fn generate_dictionary(rng: &mut rng::Rng) -> Vec<String> {
    let mut words = Vec::with_capacity(2000);
    for _ in 0..2000 {
        let syllables = 2 + usize::try_from(rng.next_u64() % 3).unwrap_or(0);
        let mut word = String::with_capacity(syllables * 2);
        for _ in 0..syllables {
            let c_idx =
                usize::try_from(rng.next_u64() % u64::try_from(CONSONANTS.len()).unwrap_or(1))
                    .unwrap_or(0);
            let v_idx = usize::try_from(rng.next_u64() % u64::try_from(VOWELS.len()).unwrap_or(1))
                .unwrap_or(0);
            word.push(CONSONANTS[c_idx] as char);
            word.push(VOWELS[v_idx] as char);
        }
        words.push(word);
    }
    words
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "word index derived from uniform float per card specification"
)]
fn pick_word_index(val: u64) -> usize {
    let u = ((val >> 11) as f64) / 9_007_199_254_740_992.0;
    let idx = (2000.0 * u * u).floor() as usize;
    idx.min(1999)
}

fn pick_filler(
    rng: &mut rng::Rng,
    dict: &[String],
    word_freq: &mut [u64; 2000],
    count: usize,
) -> String {
    let mut out = String::new();
    for i in 0..count {
        let val = rng.next_u64();
        let idx = pick_word_index(val);
        word_freq[idx] = word_freq[idx].saturating_add(1);
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&dict[idx]);
    }
    out
}

struct SourceVersionInfo {
    source_id: String,
    version: u32,
    supersedes: Option<String>,
}

struct AreaGenOutput {
    areas: Vec<GeneratedArea>,
    file: (String, Vec<u8>),
}

struct SourceGenOutput {
    files: Vec<(String, Vec<u8>)>,
    unsuperseded: Vec<(String, u32)>,
    distinct_source_ids: u64,
}

struct RegisterOutput {
    files: Vec<(String, Vec<u8>)>,
    glossary_terms: Vec<String>,
}

struct ClaimsOutput {
    file: (String, Vec<u8>),
    claims: Vec<String>,
}

struct TextGen<'a> {
    rng: &'a mut rng::Rng,
    dict: &'a [String],
    word_freq: &'a mut [u64; 2000],
}

fn generate_static_root_files(today: &str) -> Vec<(String, Vec<u8>)> {
    vec![
        ("TODAY".to_owned(), format!("{today}\n").into_bytes()),
        (
            "corpus/AGENTS.md".to_owned(),
            b"# Synthetic Corpus agent rules\n\nEvery row, source and page in this fixture is synthetic and made up for automated tests.\nKeep every register in canonical normal form and every status at its entry value unless a test fixture requires otherwise.\n".to_vec(),
        ),
        (
            "corpus/ASSISTANT.md".to_owned(),
            b"rules_version = 1\n\nSynthetic assistant rules for the mini Corpus fixture.\nUse only the supplied context pack and cite every factual statement exactly as shown.\nIf the context pack does not establish an answer, reply: Not established in the library.\n".to_vec(),
        ),
        (
            "corpus/scan-allow.toml".to_owned(),
            b"# Synthetic marking scan allowances for the mini Corpus fixture (empty in v1).\n".to_vec(),
        ),
        (
            "corpus/changes.md".to_owned(),
            b"# Corpus changes\n\nEach edition adds a section at the top, headed `## edition-NNN`, with the bullets `- Added:`, `- Changed:` and `- Verified:`. Sections run newest first. Entries carry no dates: the tag carries the date.\n".to_vec(),
        ),
    ]
}

fn generate_areas_and_corpus_toml(sizes: &Sizes) -> Result<AreaGenOutput, String> {
    let areas = build_areas(sizes.areas)?;
    let mut corpus_toml =
        String::from("schema_version = 1\n\n[assistant]\npack_budget_chars = 12000\n\n");
    for area in &areas {
        corpus_toml.push_str(&format!(
            "[[area]]\ncode = \"{}\"\nslug = \"{}\"\nname = \"Area {}\"\nsummary = \"Synthetic area {} for scale tests.\"\nsubareas = [\"core\", \"topics\"]\n\n",
            area.code, area.slug, area.code, area.code
        ));
    }
    corpus_toml.push_str(&format!(
        "[[view]]\ncode = \"XVW\"\nname = \"Scale View\"\nfilter = {{ area = \"{}\", subarea = \"core\" }}\n",
        areas[0].code
    ));
    Ok(AreaGenOutput {
        areas,
        file: ("corpus/corpus.toml".to_owned(), corpus_toml.into_bytes()),
    })
}

fn build_source_versions(count: u64) -> Result<Vec<SourceVersionInfo>, String> {
    let mut list = Vec::with_capacity(usize::try_from(count).map_err(|e| e.to_string())?);
    let mut src_num = 1u64;
    while (list.len() as u64) < count {
        let sid = format!("S-{src_num:04}");
        list.push(SourceVersionInfo {
            source_id: sid.clone(),
            version: 1,
            supersedes: None,
        });
        if src_num.is_multiple_of(3) && (list.len() as u64) < count {
            list.push(SourceVersionInfo {
                source_id: sid.clone(),
                version: 2,
                supersedes: Some(format!("{sid}-v1")),
            });
        }
        src_num = src_num.saturating_add(1);
    }
    Ok(list)
}

fn generate_sources_and_originals(
    sizes: &Sizes,
    today_trimmed: &str,
    text_gen: &mut TextGen<'_>,
) -> Result<SourceGenOutput, String> {
    let source_versions_list = build_source_versions(sizes.source_versions)?;

    let mut distinct_source_ids = BTreeSet::new();
    for sv in &source_versions_list {
        distinct_source_ids.insert(sv.source_id.clone());
    }

    let original_bytes_usize = usize::try_from(sizes.original_bytes).map_err(|e| e.to_string())?;
    let mut source_hashes: BTreeMap<String, String> = BTreeMap::new();
    let mut files = Vec::new();

    for (idx, sv) in source_versions_list.iter().enumerate() {
        let vid = format!("{}-v{}", sv.source_id, sv.version);
        if (idx as u64) < sizes.stored_originals {
            let mut text_bytes = Vec::with_capacity(original_bytes_usize);
            let prefix = format!("Synthetic fixture original for {vid}. Automated tests.\n");
            text_bytes.extend_from_slice(prefix.as_bytes());
            while text_bytes.len() < original_bytes_usize {
                let filler = pick_filler(text_gen.rng, text_gen.dict, text_gen.word_freq, 16);
                text_bytes.extend_from_slice(filler.as_bytes());
                text_bytes.push(b'\n');
            }
            text_bytes.truncate(original_bytes_usize);
            if let Some(last) = text_bytes.last_mut() {
                *last = b'\n';
            }

            let mut hasher = sha2::Sha256::new();
            hasher.update(&text_bytes);
            let sha256_hex = fieldbook_core::hex::lower(&hasher.finalize());
            source_hashes.insert(vid.clone(), sha256_hex.clone());

            files.push((
                format!("corpus/sources/public/{vid}.txt"),
                text_bytes.clone(),
            ));
            files.push((format!("corpus/sources/text/{vid}/p0001.txt"), text_bytes));
            let extraction_toml = format!(
                "tool = \"plain\"\ntool_version = \"1\"\nmethod = \"plain\"\npage_count = 1\nthin_pages = []\noriginal_sha256 = \"{sha256_hex}\"\n[pages]\np0001 = \"{sha256_hex}\"\n[locators]\n\"1\" = 1\n\"2\" = 1\n"
            );
            files.push((
                format!("corpus/sources/text/{vid}/extraction.toml"),
                extraction_toml.into_bytes(),
            ));
        } else {
            let sha256_hex = format!("{:064x}", idx + 1);
            source_hashes.insert(vid, sha256_hex);
        }
    }

    let mut sources_csv = String::from(
        "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\n",
    );
    for (idx, sv) in source_versions_list.iter().enumerate() {
        let vid = format!("{}-v{}", sv.source_id, sv.version);
        let tag = format!("synthetic-src-{}", sv.source_id.to_lowercase());
        let title = format!("Synthetic Source {}, made up for tests", sv.source_id);
        let publisher = "Synthetic Standards Board";
        let location = if (idx as u64) < sizes.stored_originals {
            format!("{vid}.txt")
        } else {
            format!("metadata-only: synthetic fixture source {vid}")
        };
        let sha256_str = source_hashes.get(&vid).map_or("", String::as_str);
        let supersedes_str = sv.supersedes.as_deref().unwrap_or("");
        sources_csv.push_str(&format!(
            "{},{},{tag},\"{title}\",{publisher},primary_law,primary_evidence,A,\"Synthetic, no restrictions\",synthetic_cost_rule,reviewed,{location},{sha256_str},{today_trimmed},{today_trimmed},{today_trimmed},{today_trimmed},active,{supersedes_str},V2,public,Synthetic fixture source for automated tests.,Synthetic source purpose.,,,,,\n",
            sv.source_id, sv.version
        ));
    }
    let norm_sources =
        normalize(Register::Sources, sources_csv.as_bytes()).map_err(|e| e.to_string())?;
    files.push((
        "corpus/registers/sources.csv".to_owned(),
        norm_sources.into_bytes(),
    ));

    let mut unsuperseded = Vec::new();
    let mut seen_sids = BTreeMap::new();
    for sv in &source_versions_list {
        seen_sids
            .entry(sv.source_id.clone())
            .and_modify(|v: &mut u32| *v = (*v).max(sv.version))
            .or_insert(sv.version);
    }
    for (sid, max_v) in seen_sids {
        unsuperseded.push((sid, max_v));
    }

    let distinct_source_ids =
        u64::try_from(distinct_source_ids.len()).map_err(|e| e.to_string())?;

    Ok(SourceGenOutput {
        files,
        unsuperseded,
        distinct_source_ids,
    })
}

fn generate_questions_conflicts_glossary(
    sizes: &Sizes,
    areas: &[GeneratedArea],
) -> Result<RegisterOutput, String> {
    let mut files = Vec::new();

    // 1. Questions register.
    let mut questions_csv = String::from(
        "question_id,area,question,owner_role,blocks,why_it_matters,default_assumption,work_can_continue,day_one,status,answer,answered_on,adoption\n",
    );
    for q in 0..sizes.questions {
        let qid = format!("Q-{:03}", q + 1);
        let area_idx = usize::try_from(q % sizes.areas).unwrap_or(0);
        let area_code = &areas[area_idx].code;
        let q_text = format!("Which synthetic rule applies to question {}?", q + 1);
        questions_csv.push_str(&format!(
            "{qid},{area_code},\"{q_text}\",,,,,,false,open,,,\n"
        ));
    }
    let norm_questions =
        normalize(Register::Questions, questions_csv.as_bytes()).map_err(|e| e.to_string())?;
    files.push((
        "corpus/registers/questions.csv".to_owned(),
        norm_questions.into_bytes(),
    ));

    // 2. Conflicts register.
    let mut conflicts_csv = String::from(
        "conflict_id,area,topic,positions,resolution,rationale,evidence,resolved_on,status,claim_ids,question_id\n",
    );
    for c in 0..sizes.conflicts {
        let xcid = format!("XC-{:02}", c + 1);
        let qid = format!("Q-{:03}", c + 1);
        let claim1_idx = 2 * c;
        let claim2_idx = 2 * c + 1;
        let area1_idx = usize::try_from(claim1_idx % sizes.areas).unwrap_or(0);
        let area_code = &areas[area1_idx].code;
        let seq1 = (claim1_idx / sizes.areas) + 1;
        let seq2 = (claim2_idx / sizes.areas) + 1;
        let cid1 = format!("CL-{}-{:04}", area_code.to_ascii_uppercase(), seq1);
        let area2_idx = usize::try_from(claim2_idx % sizes.areas).unwrap_or(0);
        let area2_code = &areas[area2_idx].code;
        let cid2 = format!("CL-{}-{:04}", area2_code.to_ascii_uppercase(), seq2);
        let topic = format!("Synthetic conflict {} topic for tests", c + 1);
        let pos = format!(
            "Synthetic position A for conflict {}|Synthetic position B for conflict {}",
            c + 1,
            c + 1
        );
        conflicts_csv.push_str(&format!(
            "{xcid},{area_code},\"{topic}\",\"{pos}\",,,,,unresolved,{cid1}|{cid2},{qid}\n"
        ));
    }
    let norm_conflicts =
        normalize(Register::Conflicts, conflicts_csv.as_bytes()).map_err(|e| e.to_string())?;
    files.push((
        "corpus/registers/conflicts.csv".to_owned(),
        norm_conflicts.into_bytes(),
    ));

    // 3. Glossary register.
    let mut glossary_terms =
        Vec::with_capacity(usize::try_from(sizes.glossary_terms).map_err(|e| e.to_string())?);
    let mut glossary_csv = String::from(
        "term_id,label,definition,boundary_rule,owner_area,source_ref,definition_status,aliases,superseded_by\n",
    );
    for t in 0..sizes.glossary_terms {
        let term_id = format!("term_{:04}", t + 1);
        let label = format!("term {:04}", t + 1);
        let area_idx = usize::try_from(t % sizes.areas).unwrap_or(0);
        let owner_area = &areas[area_idx].code;
        let def = format!("Synthetic definition for term {}.", t + 1);
        let boundary = format!("Synthetic boundary rule for term {}.", t + 1);
        glossary_csv.push_str(&format!(
            "{term_id},{label},\"{def}\",\"{boundary}\",{owner_area},,general_practice,,\n"
        ));
        glossary_terms.push(term_id);
    }
    let norm_glossary =
        normalize(Register::Glossary, glossary_csv.as_bytes()).map_err(|e| e.to_string())?;
    files.push((
        "corpus/registers/glossary.csv".to_owned(),
        norm_glossary.into_bytes(),
    ));

    Ok(RegisterOutput {
        files,
        glossary_terms,
    })
}

fn generate_claims_register(
    sizes: &Sizes,
    areas: &[GeneratedArea],
    unsuperseded: &[(String, u32)],
    today_trimmed: &str,
) -> Result<ClaimsOutput, String> {
    let mut claims = Vec::with_capacity(usize::try_from(sizes.claims).map_err(|e| e.to_string())?);
    let mut claims_csv = String::from(
        "claim_id,area,kind,statement,as_of,source_id,version,locator_kind,locator,quote,consequential,drafted_by,verification,applicability,adoption,data_origin,volatility,value,unit,effective_from,effective_to,instrument,citation_codified,citation_rfo,edition_date,text_status,applies_to,evidence_grade,verified_on,verified_by_role,supersedes,superseded_by,conflict_id,question_id,note\n",
    );

    for i in 0..sizes.claims {
        let area_idx = usize::try_from(i % sizes.areas).unwrap_or(0);
        let area = &areas[area_idx];
        let seq = (i / sizes.areas) + 1;
        let claim_id = format!("CL-{}-{:04}", area.code.to_ascii_uppercase(), seq);
        let (src_id, ver) =
            &unsuperseded[usize::try_from(i % (unsuperseded.len() as u64)).unwrap_or(0)];

        let conflict_id = if i < sizes.conflicts * 2 {
            let c_num = (i / 2) + 1;
            Some(format!("XC-{c_num:02}"))
        } else {
            None
        };

        let statement = format!("Synthetic fact statement for {claim_id}.");
        let xcid_str = conflict_id.as_deref().unwrap_or("");
        let ver_str = ver.to_string();
        let cells = [
            claim_id.as_str(),
            area.code.as_str(),
            "fact",
            &statement,
            today_trimmed,
            src_id.as_str(),
            &ver_str,
            "no_fine_locator",
            "",
            "",
            "false",
            "codex",
            "unchecked",
            "pending_company_confirmation",
            "proposed",
            "observed",
            "V2",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            xcid_str,
            "",
            "Synthetic claim for scale fixture tests.",
        ];
        for (col_idx, cell) in cells.iter().enumerate() {
            if col_idx > 0 {
                claims_csv.push(',');
            }
            if cell.contains(',') || cell.contains('"') {
                claims_csv.push('"');
                claims_csv.push_str(&cell.replace('"', "\"\""));
                claims_csv.push('"');
            } else {
                claims_csv.push_str(cell);
            }
        }
        claims_csv.push('\n');

        claims.push(claim_id);
    }
    let norm_claims =
        normalize(Register::Claims, claims_csv.as_bytes()).map_err(|e| e.to_string())?;

    Ok(ClaimsOutput {
        file: (
            "corpus/registers/claims.csv".to_owned(),
            norm_claims.into_bytes(),
        ),
        claims,
    })
}

fn render_concept_body(
    page_id: &str,
    claim_a: &str,
    claim_b: &str,
    today_trimmed: &str,
    filler: &str,
) -> String {
    let mut body = String::new();
    body.push_str("## Purpose\n\n");
    body.push_str(&format!(
        "Synthetic purpose for concept page {page_id}. {filler}\n\n"
    ));
    body.push_str("## The rule\n\n");
    body.push_str(&format!(
        "Synthetic rule statement for concept page {page_id} [@{claim_a}]. {filler}\n\n"
    ));
    body.push_str("## How it works\n\n");
    body.push_str(&format!(
        "Synthetic mechanism for concept page {page_id} [@{claim_b}]. {filler}\n\n"
    ));
    body.push_str("## Worked example\n\n");
    body.push_str("Synthetic example. The numbers are made up.\n\n");
    body.push_str(&format!(
        "Synthetic setup and working for concept page {page_id}. {filler}\n\n"
    ));
    body.push_str("## Never confuse\n\n");
    body.push_str(&format!(
        "Synthetic distinction for concept page {page_id}. {filler}\n\n"
    ));
    body.push_str("## Company calibration\n\n");
    body.push_str(&format!(
        "Synthetic calibration note for concept page {page_id}. {filler}\n\n"
    ));
    body.push_str("## Sources\n\n");
    body.push_str("## Change note\n\n");
    body.push_str(&format!(
        "- NEW_KNOWLEDGE, {today_trimmed}: Added synthetic concept page.\n"
    ));
    body
}

fn render_procedure_body(
    page_id: &str,
    claim_a: &str,
    claim_b: &str,
    today_trimmed: &str,
    filler: &str,
) -> String {
    let mut body = String::new();
    body.push_str("## Purpose\n\n");
    body.push_str(&format!(
        "Synthetic purpose for procedure page {page_id}. {filler}\n\n"
    ));
    body.push_str("## The rule\n\n");
    body.push_str(&format!(
        "Synthetic rule statement for procedure page {page_id} [@{claim_a}]. {filler}\n\n"
    ));
    body.push_str("## How it works\n\n");
    body.push_str(&format!(
        "1. Open synthetic step log for {page_id} [@{claim_b}].\n2. Verify synthetic procedure outcome for {page_id}. {filler}\n\n"
    ));
    body.push_str("## Worked example\n\n");
    body.push_str("Synthetic example. The numbers are made up.\n\n");
    body.push_str(&format!(
        "Synthetic setup and working for procedure page {page_id}. {filler}\n\n"
    ));
    body.push_str("## Never confuse\n\n");
    body.push_str(&format!(
        "Synthetic distinction for procedure page {page_id}. {filler}\n\n"
    ));
    body.push_str("## Company calibration\n\n");
    body.push_str(&format!(
        "Synthetic calibration question for procedure page {page_id}. {filler}\n\n"
    ));
    body.push_str("## Sources\n\n");
    body.push_str("## Change note\n\n");
    body.push_str(&format!(
        "- NEW_KNOWLEDGE, {today_trimmed}: Added synthetic procedure page.\n"
    ));
    body
}

fn render_brief_body(
    page_id: &str,
    claim_a: &str,
    claim_b: &str,
    qid: &str,
    filler: &str,
) -> String {
    let mut body = String::new();
    body.push_str("## Question\n\n");
    body.push_str(&format!(
        "Synthetic question for brief {page_id}. {filler}\n\n"
    ));
    body.push_str("## Recommendation\n\n");
    body.push_str(&format!(
        "Synthetic recommendation for brief {page_id}. {filler}\n\n"
    ));
    body.push_str("## Evidence\n\n");
    body.push_str(&format!(
        "- Synthetic evidence alpha [@{claim_a}].\n- Synthetic evidence beta [@{claim_b}].\n\n"
    ));
    body.push_str("## Options\n\n");
    body.push_str(&format!(
        "Synthetic option considerations for brief {page_id}. {filler}\n\n"
    ));
    body.push_str("## Unknowns\n\n");
    body.push_str(&format!("- Synthetic unknown item [@{qid}].\n\n"));
    body.push_str("## Next action\n\n");
    body.push_str(&format!(
        "Synthetic next action for brief {page_id}. {filler}\n"
    ));
    body
}

fn generate_pages(
    sizes: &Sizes,
    areas: &[GeneratedArea],
    claims: &[String],
    glossary_terms: &[String],
    today_trimmed: &str,
    text_gen: &mut TextGen<'_>,
) -> (Vec<(String, Vec<u8>)>, u64) {
    let words_ceiling = usize::try_from(sizes.words_per_section).unwrap_or(0);
    let mut total_sections = 0u64;
    let mut files = Vec::with_capacity(usize::try_from(sizes.pages).unwrap_or(0));

    for p in 0..sizes.pages {
        let area_idx = usize::try_from(p % sizes.areas).unwrap_or(0);
        let area = &areas[area_idx];
        let subarea_idx =
            usize::try_from((p / sizes.areas) % (area.subareas.len() as u64)).unwrap_or(0);
        let subarea = &area.subareas[subarea_idx];
        let seq = (p / sizes.areas) + 1;
        let page_id = format!("{}-p{:04}", area.code, seq);

        let page_type = match p % 3 {
            0 => PageType::Concept,
            1 => PageType::Procedure,
            _ => PageType::Brief,
        };

        let claim_a_idx = usize::try_from((2 * p) % sizes.claims).unwrap_or(0);
        let claim_b_idx = usize::try_from((2 * p + 1) % sizes.claims).unwrap_or(0);
        let claim_a = &claims[claim_a_idx];
        let claim_b = if claim_a_idx == claim_b_idx {
            &claims[(claim_a_idx + 1) % claims.len()]
        } else {
            &claims[claim_b_idx]
        };

        let term_id = &glossary_terms[usize::try_from(p % sizes.glossary_terms).unwrap_or(0)];
        let stage = 1 + (p % 8);

        let body = match page_type {
            PageType::Concept => {
                total_sections = total_sections.saturating_add(7);
                let filler = pick_filler(
                    text_gen.rng,
                    text_gen.dict,
                    text_gen.word_freq,
                    words_ceiling,
                );
                render_concept_body(&page_id, claim_a, claim_b, today_trimmed, &filler)
            }
            PageType::Procedure => {
                total_sections = total_sections.saturating_add(7);
                let filler = pick_filler(
                    text_gen.rng,
                    text_gen.dict,
                    text_gen.word_freq,
                    words_ceiling,
                );
                render_procedure_body(&page_id, claim_a, claim_b, today_trimmed, &filler)
            }
            PageType::Brief => {
                total_sections = total_sections.saturating_add(6);
                let brief_filler_limit = words_ceiling.min(30);
                let filler = pick_filler(
                    text_gen.rng,
                    text_gen.dict,
                    text_gen.word_freq,
                    brief_filler_limit,
                );
                let qid = format!("Q-{:03}", (p % sizes.questions) + 1);
                render_brief_body(&page_id, claim_a, claim_b, &qid, &filler)
            }
            _ => String::new(),
        };

        let type_str = page_type.as_str();
        let front_matter = format!(
            "+++\nid = \"{page_id}\"\ntitle = \"Synthetic {type_str} {page_id}\"\narea = \"{}\"\nsubarea = \"{subarea}\"\ntype = \"{type_str}\"\nstage = {stage}\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = {today_trimmed}\nhandling = [\"public\"]\nterms = [\"{term_id}\"]\n+++\n\n",
            area.code
        );

        let full_page = format!("{front_matter}{body}");
        files.push((
            format!("corpus/pages/{}/{}/{page_id}.md", area.slug, subarea),
            full_page.into_bytes(),
        ));
    }

    (files, total_sections)
}

/// Generates a synthetic Corpus in memory according to `base` scaled by `m`.
pub fn generate(base: &Base, m: u64, today: &str) -> Result<Generated, String> {
    let sizes = validate(base, m)?;
    let today_trimmed = today.trim();

    let mut rng = rng::Rng::new(sizes.seed);
    let dict = generate_dictionary(&mut rng);
    let mut word_freq = [0u64; 2000];
    let mut text_gen = TextGen {
        rng: &mut rng,
        dict: &dict,
        word_freq: &mut word_freq,
    };

    let mut files = Vec::new();

    // 1. Static root files.
    files.extend(generate_static_root_files(today_trimmed));

    // 2. Areas and corpus.toml.
    let areas_out = generate_areas_and_corpus_toml(&sizes)?;
    files.push(areas_out.file);

    // 3. Source versions and stored originals.
    let sources_out = generate_sources_and_originals(&sizes, today_trimmed, &mut text_gen)?;
    files.extend(sources_out.files);

    // 4. Questions, conflicts, and glossary registers.
    let registers_out = generate_questions_conflicts_glossary(&sizes, &areas_out.areas)?;
    files.extend(registers_out.files);

    // 5. Claims register.
    let claims_out = generate_claims_register(
        &sizes,
        &areas_out.areas,
        &sources_out.unsuperseded,
        today_trimmed,
    )?;
    files.push(claims_out.file);

    // 6. Pages.
    let (page_files, total_sections) = generate_pages(
        &sizes,
        &areas_out.areas,
        &claims_out.claims,
        &registers_out.glossary_terms,
        today_trimmed,
        &mut text_gen,
    );
    files.extend(page_files);

    // Sort files in byte order.
    files.sort_by(|a, b| a.0.cmp(&b.0));

    // Determine query word (most frequent generated pseudo-word).
    let most_frequent_idx = word_freq
        .iter()
        .enumerate()
        .max_by_key(|&(idx, &count)| (count, std::cmp::Reverse(idx)))
        .map_or(0, |(idx, _)| idx);
    let query = dict[most_frequent_idx].clone();

    let corpus_bytes: u64 = files.iter().map(|(_, b)| b.len() as u64).sum();
    let files_count = u64::try_from(files.len()).map_err(|e| e.to_string())?;

    let counts = Counts {
        areas: sizes.areas,
        pages: sizes.pages,
        sections: total_sections,
        claims: sizes.claims,
        sources: sizes.source_versions,
        source_ids: sources_out.distinct_source_ids,
        stored_originals: sizes.stored_originals,
        questions: sizes.questions,
        conflicts: sizes.conflicts,
        terms: sizes.glossary_terms,
        files: files_count,
        corpus_bytes,
    };

    Ok(Generated {
        files,
        counts,
        query,
    })
}

/// A row of scale measurements for a given Corpus multiplier size.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Multiplier size `m` (e.g. 1, 5, 20).
    pub size: u64,
    /// Number of document units in search index, or `None` if build was stopped.
    pub units: Option<u64>,
    /// Total corpus bytes divided by 1000, rounded to nearest integer.
    pub corpus_kb: u64,
    /// Duration of `check` in seconds.
    pub check_s: f64,
    /// Duration of `check corpus/pages` in seconds.
    pub check_path_s: f64,
    /// Duration of `build` in seconds.
    pub build_s: f64,
    /// Whether the build was stopped by FB0605.
    pub build_stopped: bool,
    /// Duration of `search` in whole milliseconds.
    pub search_ms: u64,
    /// Size of `search-index.js` in kilobytes (1000 bytes) rounded, or `None` if build was stopped.
    pub index_kb: Option<u64>,
    /// Peak memory (VmHWM) in megabytes (1,000,000 bytes) rounded, or `None` on non-Linux.
    pub rss_mb: Option<u64>,
}

/// Generate the NFR-F-04 check budget line for size 1x or 20x.
pub fn nfr_line(size: u64, millis: u64) -> Option<String> {
    let (budget_ms, budget_str) = match size {
        1 => (5000u64, "5 s"),
        20 => (60000u64, "60 s"),
        _ => return None,
    };
    let verdict = if millis <= budget_ms {
        "within"
    } else {
        "over"
    };
    let secs = millis / 1000;
    let rem_ms = millis % 1000;
    Some(format!(
        "NFR-F-04 check {size}x: {secs}.{rem_ms:03} s, budget {budget_str}, {verdict}"
    ))
}

/// Parse `VmHWM` in kB from `/proc/<pid>/status` content.
pub fn vmhwm_kb(status: &str) -> Option<u64> {
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let trimmed = rest.trim();
            let num_str = trimmed
                .strip_suffix("kB")
                .or_else(|| trimmed.strip_suffix("kb"))
                .unwrap_or(trimmed)
                .trim();
            if let Ok(val) = num_str.parse::<u64>() {
                return Some(val);
            }
        }
    }
    None
}

/// Format the full scale results table and NFR evaluation.
pub fn render_table(base_arg: &str, seed: u64, rows: &[Row]) -> String {
    let mut out = String::new();
    out.push_str(&format!("scale base={base_arg} seed={seed}\n"));

    const HEADERS: [&str; 9] = [
        "size",
        "units",
        "corpus_kb",
        "check_s",
        "check_path_s",
        "build_s",
        "search_ms",
        "index_kb",
        "rss_mb",
    ];

    let mut formatted_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let size_str = format!("{}x", row.size);
        let units_str = row.units.map_or_else(|| "-".to_string(), |u| u.to_string());
        let corpus_str = row.corpus_kb.to_string();
        let check_str = format!("{:.2}", row.check_s);
        let check_path_str = format!("{:.2}", row.check_path_s);
        let build_str = if row.build_stopped {
            format!("{:.2}*", row.build_s)
        } else {
            format!("{:.2}", row.build_s)
        };
        let search_str = row.search_ms.to_string();
        let index_str = row
            .index_kb
            .map_or_else(|| "-".to_string(), |i| i.to_string());
        let rss_str = row
            .rss_mb
            .map_or_else(|| "n/a".to_string(), |r| r.to_string());

        formatted_rows.push([
            size_str,
            units_str,
            corpus_str,
            check_str,
            check_path_str,
            build_str,
            search_str,
            index_str,
            rss_str,
        ]);
    }

    let mut widths = [0usize; 9];
    for (i, h) in HEADERS.iter().enumerate() {
        if let Some(w) = widths.get_mut(i) {
            *w = h.len();
        }
    }
    for row in &formatted_rows {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                *w = (*w).max(cell.len());
            }
        }
    }

    let mut header_line = String::new();
    for (i, h) in HEADERS.iter().enumerate() {
        if i > 0 {
            header_line.push_str("  ");
        }
        let w = widths.get(i).copied().unwrap_or(0);
        header_line.push_str(&format!("{:>width$}", h, width = w));
    }
    out.push_str(&header_line);
    out.push('\n');

    for row in &formatted_rows {
        let mut row_line = String::new();
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                row_line.push_str("  ");
            }
            let w = widths.get(i).copied().unwrap_or(0);
            row_line.push_str(&format!("{:>width$}", cell, width = w));
        }
        out.push_str(&row_line);
        out.push('\n');
    }

    if rows.iter().any(|r| r.build_stopped) {
        out.push_str(
            "* build stopped by FB0605 (a data file over the cap); the time is to the stop.\n",
        );
    }

    for target_size in [1u64, 20u64] {
        if let Some(row) = rows.iter().find(|r| r.size == target_size) {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "milliseconds rounded from seconds duration"
            )]
            let millis = (row.check_s * 1000.0).round() as u64;
            if let Some(line) = nfr_line(target_size, millis) {
                out.push_str(&line);
                out.push('\n');
            }
        }
    }

    out
}

struct DirGuard {
    path: PathBuf,
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        remove_dir_all_writable(&self.path);
    }
}

#[expect(
    clippy::permissions_set_readonly_false,
    reason = "clear read-only flag before directory removal across platforms"
)]
fn make_writable(path: &Path) {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        let _ = std::fs::set_permissions(path, permissions);
    }
}

fn walk_and_set_writable(dir: &Path) {
    make_writable(dir);
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_and_set_writable(&path);
            } else {
                make_writable(&path);
            }
        }
    }
}

fn remove_dir_all_writable(dir: &Path) {
    if !dir.exists() {
        return;
    }
    walk_and_set_writable(dir);
    let _ = std::fs::remove_dir_all(dir);
}

struct CommandResult {
    status: std::process::ExitStatus,
    duration: std::time::Duration,
    stdout: String,
    stderr: String,
    #[cfg_attr(
        not(target_os = "linux"),
        expect(dead_code, reason = "peak RSS only read on Linux")
    )]
    max_vmhwm_kb: Option<u64>,
}

fn run_timed_command(bin: &Path, args: &[&str], cwd: &Path) -> Result<CommandResult, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    let start = Instant::now();
    let mut child = Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn `{}`: {e}", bin.display()))?;

    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture child stdout".to_string())?;
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });

    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture child stderr".to_string())?;
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    #[cfg(target_os = "linux")]
    let pid = child.id();
    #[cfg(target_os = "linux")]
    let status_path = format!("/proc/{pid}/status");
    #[cfg(target_os = "linux")]
    let mut peak_kb: Option<u64> = None;

    let status = loop {
        #[cfg(target_os = "linux")]
        if let Some(kb) = std::fs::read_to_string(&status_path)
            .ok()
            .and_then(|s| vmhwm_kb(&s))
        {
            peak_kb = Some(peak_kb.map_or(kb, |prev| prev.max(kb)));
        }

        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
            Err(e) => return Err(format!("failed waiting for child `{}`: {e}", bin.display())),
        }
    };

    let duration = start.elapsed();

    let stdout_bytes: Vec<u8> = stdout_thread.join().unwrap_or_default();
    let stderr_bytes: Vec<u8> = stderr_thread.join().unwrap_or_default();

    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();

    #[cfg(target_os = "linux")]
    let max_vmhwm_kb = peak_kb;
    #[cfg(not(target_os = "linux"))]
    let max_vmhwm_kb = None;

    Ok(CommandResult {
        status,
        duration,
        stdout,
        stderr,
        max_vmhwm_kb,
    })
}

fn print_failure_output(cmd_label: &str, m: u64, code: i32, stdout: &str, stderr: &str) {
    eprintln!("scale: {cmd_label} failed on the generated Corpus ({m}x), exit {code}");
    let combined = format!("{stdout}{stderr}");
    for line in combined.lines().take(20) {
        eprintln!("{line}");
    }
}

fn parse_sizes(s: &str) -> Result<Vec<u64>, String> {
    if s.trim().is_empty() {
        return Err("sizes cannot be empty".to_string());
    }
    let mut list = Vec::new();
    let mut seen = BTreeSet::new();
    for part in s.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            return Err("empty size in `--sizes` list".to_string());
        }
        let n: u64 = trimmed
            .parse()
            .map_err(|_| format!("invalid size `{trimmed}` in `--sizes`: must be an integer"))?;
        if n == 0 {
            return Err("size must be at least 1".to_string());
        }
        if !seen.insert(n) {
            return Err(format!("repeated size `{n}` in `--sizes`"));
        }
        list.push(n);
    }
    Ok(list)
}

const USAGE: &str = "usage: cargo xtask scale [--sizes 1,5,20] [--base FILE] [--bin PATH]";

#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    sizes: Option<Vec<u64>>,
    base: Option<String>,
    bin: Option<String>,
}

enum Failure {
    Exit(u8, String),
    Command {
        what: &'static str,
        m: u64,
        code: i32,
        stdout: String,
        stderr: String,
    },
}

impl Failure {
    fn emit(&self) -> u8 {
        match self {
            Self::Exit(code, msg) => {
                if !msg.is_empty() {
                    eprintln!("{msg}");
                }
                *code
            }
            Self::Command {
                what,
                m,
                code,
                stdout,
                stderr,
            } => {
                print_failure_output(what, *m, *code, stdout, stderr);
                1
            }
        }
    }
}

fn take_flag_value<S: AsRef<str>>(
    args: &[S],
    i: &mut usize,
    flag: &str,
    slot: &mut Option<String>,
) -> Result<bool, String> {
    let arg = match args.get(*i) {
        Some(a) => a.as_ref(),
        None => return Ok(false),
    };
    let prefix = format!("{flag}=");
    if arg == flag {
        if slot.is_some() {
            return Err(format!("repeated `{flag}` flag"));
        }
        *i += 1;
        match args.get(*i) {
            Some(a) => {
                *slot = Some(a.as_ref().to_string());
                Ok(true)
            }
            None => Err(format!("missing value for `{flag}`")),
        }
    } else if let Some(rest) = arg.strip_prefix(&prefix) {
        if slot.is_some() {
            return Err(format!("repeated `{flag}` flag"));
        }
        *slot = Some(rest.to_string());
        Ok(true)
    } else {
        Ok(false)
    }
}

fn parse_args<S: AsRef<str>>(args: &[S]) -> Result<Args, String> {
    let mut raw_sizes = None;
    let mut sizes = None;
    let mut base = None;
    let mut bin = None;

    let mut i = 0;
    while i < args.len() {
        if take_flag_value(args, &mut i, "--sizes", &mut raw_sizes)? {
            let val = raw_sizes.as_deref().unwrap_or_default();
            sizes = Some(parse_sizes(val)?);
        } else if take_flag_value(args, &mut i, "--base", &mut base)? {
            // base parsed
        } else if take_flag_value(args, &mut i, "--bin", &mut bin)? {
            // bin parsed
        } else {
            return Err(format!("unknown argument `{}`", args[i].as_ref()));
        }
        i += 1;
    }

    Ok(Args { sizes, base, bin })
}

fn resolve_binary(root: &Path, bin_arg: Option<&str>) -> Result<PathBuf, Failure> {
    if let Some(path_str) = bin_arg {
        let p = PathBuf::from(path_str);
        let path = if p.is_absolute() {
            p
        } else {
            std::env::current_dir().map_or_else(|_| p.clone(), |c| c.join(&p))
        };
        Ok(path)
    } else {
        eprintln!("scale: building fieldbook release binary with cargo...");
        let build_status = std::process::Command::new("cargo")
            .args([
                "build",
                "--locked",
                "--release",
                "-p",
                fieldbook_core::strings::binary(),
            ])
            .current_dir(root)
            .status();
        match build_status {
            Ok(status) if status.success() => {
                if cfg!(windows) {
                    Ok(root.join("target/release/fieldbook.exe"))
                } else {
                    Ok(root.join("target/release/fieldbook"))
                }
            }
            Ok(status) => {
                let code = status.code().unwrap_or(3);
                Err(Failure::Exit(
                    3,
                    format!("scale: cargo release build failed with exit code {code}"),
                ))
            }
            Err(e) => Err(Failure::Exit(
                3,
                format!("scale: failed to run cargo release build: {e}"),
            )),
        }
    }
}

fn parse_search_index(folder: &Path) -> Result<(u64, u64), Failure> {
    let index_file = folder.join("site/edition/assets/search-index.js");
    let meta = std::fs::metadata(&index_file).map_err(|e| {
        Failure::Exit(
            1,
            format!("scale: failed to read `{}`: {e}", index_file.display()),
        )
    })?;
    let index_kb = meta.len().saturating_add(500) / 1000;

    let content = std::fs::read_to_string(&index_file).map_err(|e| {
        Failure::Exit(
            1,
            format!("scale: failed to read `{}`: {e}", index_file.display()),
        )
    })?;
    let content_trimmed = content.trim();
    let Some(json_str) = content_trimmed
        .strip_prefix("window.FB_INDEX = ")
        .and_then(|s| s.strip_suffix(';'))
    else {
        return Err(Failure::Exit(
            1,
            "scale: unexpected format in search-index.js".to_string(),
        ));
    };
    let val: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        Failure::Exit(
            1,
            format!("scale: failed to parse JSON in search-index.js: {e}"),
        )
    })?;
    let doc_count = match val.get("docs").and_then(|d| d.as_array()) {
        Some(arr) => u64::try_from(arr.len()).unwrap_or(u64::MAX),
        None => {
            return Err(Failure::Exit(
                1,
                "scale: missing `docs` array in search-index.js".to_string(),
            ));
        }
    };
    Ok((doc_count, index_kb))
}

fn write_generated_corpus(folder: &Path, generated: &Generated) -> Result<(), Failure> {
    for (rel_path, bytes) in &generated.files {
        let full_path = folder.join(rel_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Failure::Exit(
                    3,
                    format!(
                        "scale: failed to create directory `{}`: {e}",
                        parent.display()
                    ),
                )
            })?;
        }
        std::fs::write(&full_path, bytes).map_err(|e| {
            Failure::Exit(
                3,
                format!("scale: failed to write file `{}`: {e}", full_path.display()),
            )
        })?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn compute_rss_mb(peaks: [Option<u64>; 4]) -> Option<u64> {
    let max_kb = peaks.into_iter().flatten().max();
    max_kb.map(|kb| kb.saturating_mul(1024).saturating_add(500_000) / 1_000_000)
}

#[cfg(not(target_os = "linux"))]
fn compute_rss_mb(_peaks: [Option<u64>; 4]) -> Option<u64> {
    None
}

fn measure_size(
    bin: &Path,
    base: &Base,
    m: u64,
    temp_dir: &Path,
    pid: u32,
) -> Result<Row, Failure> {
    let folder = temp_dir.join(format!("fieldbook-scale-{pid}-{m}x"));
    std::fs::create_dir(&folder).map_err(|e| {
        Failure::Exit(
            3,
            format!(
                "scale: failed to create temporary folder `{}`: {e}",
                folder.display()
            ),
        )
    })?;
    let _guard = DirGuard {
        path: folder.clone(),
    };

    let generated = generate(base, m, TODAY_TEXT)
        .map_err(|e| Failure::Exit(1, format!("scale: generation failed for size {m}x: {e}")))?;
    write_generated_corpus(&folder, &generated)?;

    let today_arg = TODAY_TEXT.trim();

    // 1. check --strict --today <TODAY>
    eprintln!("scale: [{m}x] running check --strict...");
    let res1 = run_timed_command(bin, &["check", "--strict", "--today", today_arg], &folder)
        .map_err(|e| Failure::Exit(3, format!("scale: {e}")))?;
    if !res1.status.success() {
        let code = res1.status.code().unwrap_or(1);
        return Err(Failure::Command {
            what: "check",
            m,
            code,
            stdout: res1.stdout,
            stderr: res1.stderr,
        });
    }

    // 2. check --strict --today <TODAY> corpus/pages
    eprintln!("scale: [{m}x] running check --strict corpus/pages...");
    let res2 = run_timed_command(
        bin,
        &["check", "--strict", "--today", today_arg, "corpus/pages"],
        &folder,
    )
    .map_err(|e| Failure::Exit(3, format!("scale: {e}")))?;
    if !res2.status.success() {
        let code = res2.status.code().unwrap_or(1);
        return Err(Failure::Command {
            what: "check corpus/pages",
            m,
            code,
            stdout: res2.stdout,
            stderr: res2.stderr,
        });
    }

    // 3. build --out site
    eprintln!("scale: [{m}x] running build --out site...");
    let res3 = run_timed_command(bin, &["build", "--out", "site"], &folder)
        .map_err(|e| Failure::Exit(3, format!("scale: {e}")))?;
    let mut build_stopped = false;
    let mut units = None;
    let mut index_kb = None;

    if res3.status.success() {
        let (u, ikb) = parse_search_index(&folder)?;
        units = Some(u);
        index_kb = Some(ikb);
    } else {
        let combined_build = format!("{}{}", res3.stdout, res3.stderr);
        let code = res3.status.code().unwrap_or(1);
        if code == 1 && combined_build.contains("FB0605") {
            build_stopped = true;
        } else {
            return Err(Failure::Command {
                what: "build",
                m,
                code,
                stdout: res3.stdout,
                stderr: res3.stderr,
            });
        }
    }

    // 4. search <query>
    eprintln!("scale: [{m}x] running search {}...", generated.query);
    let res4 = run_timed_command(bin, &["search", &generated.query], &folder)
        .map_err(|e| Failure::Exit(3, format!("scale: {e}")))?;
    if !res4.status.success() {
        let code = res4.status.code().unwrap_or(1);
        return Err(Failure::Command {
            what: "search",
            m,
            code,
            stdout: res4.stdout,
            stderr: res4.stderr,
        });
    }

    let rss_mb = compute_rss_mb([
        res1.max_vmhwm_kb,
        res2.max_vmhwm_kb,
        res3.max_vmhwm_kb,
        res4.max_vmhwm_kb,
    ]);

    let corpus_kb = generated.counts.corpus_bytes.saturating_add(500) / 1000;
    let search_ms = u64::try_from(res4.duration.as_millis()).unwrap_or(u64::MAX);

    Ok(Row {
        size: m,
        units,
        corpus_kb,
        check_s: res1.duration.as_secs_f64(),
        check_path_s: res2.duration.as_secs_f64(),
        build_s: res3.duration.as_secs_f64(),
        build_stopped,
        search_ms,
        index_kb,
        rss_mb,
    })
}

/// Execute the scale command with given arguments, appending stdout to `out`.
pub fn run<S: AsRef<str>>(root: &Path, args: &[S], out: &mut String) -> u8 {
    let parsed_args = match parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("scale: {e}\n{USAGE}");
            return 2;
        }
    };

    let base_str = parsed_args.base.as_deref().unwrap_or("scale/base.toml");
    let base_path = root.join(base_str);
    let base_content = match std::fs::read_to_string(&base_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "scale: failed to read base file `{}`: {e}",
                base_path.display()
            );
            return 2;
        }
    };
    let base = match Base::parse(&base_content) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("scale: invalid base file `{}`: {e}", base_path.display());
            return 2;
        }
    };

    let sizes = parsed_args.sizes.unwrap_or_else(|| vec![1, 5, 20]);
    for &m in &sizes {
        if let Err(e) = validate(&base, m) {
            eprintln!("scale: validation failed for size {m}x: {e}");
            return 2;
        }
    }

    let bin = match resolve_binary(root, parsed_args.bin.as_deref()) {
        Ok(b) => b,
        Err(f) => return f.emit(),
    };

    let mut rows = Vec::with_capacity(sizes.len());
    let pid = std::process::id();
    let temp_dir = std::env::temp_dir();

    for &m in &sizes {
        match measure_size(&bin, &base, m, &temp_dir, pid) {
            Ok(row) => rows.push(row),
            Err(f) => return f.emit(),
        }
    }

    out.push_str(&render_table(base_str, base.seed, &rows));
    0
}

/// Entry point for `cargo xtask scale`.
pub fn command<S: AsRef<str>>(root: &Path, args: &[S]) -> ExitCode {
    let mut out = String::new();
    let code = run(root, args, &mut out);
    if !out.is_empty() {
        print!("{out}");
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fieldbook_core::check::{Host, Options, Tree, run as check_run};
    use fieldbook_core::date::CivilDate;

    const BASE_TOML: &str = include_str!("../../scale/base.toml");
    const TINY_TOML: &str = include_str!("../tests/scale-tiny.toml");

    #[test]
    fn base_parses_and_rejects() {
        let base = Base::parse(BASE_TOML).expect("scale/base.toml must parse cleanly");
        assert!(base.seed > 0);
        assert!(base.areas > 0);
        assert!(base.pages > 0);
        assert!(base.words_per_section > 0);
        assert!(base.source_versions > 0);
        assert!(base.stored_originals > 0);
        assert!(base.original_bytes > 0);
        assert!(base.claims > 0);
        assert!(base.questions > 0);
        assert!(base.conflicts > 0);
        assert!(base.glossary_terms > 0);

        // 1. Missing key
        let missing = BASE_TOML.replace("seed = 1\n", "");
        let err = Base::parse(&missing).expect_err("missing key must fail");
        assert!(
            err.contains("seed"),
            "error message must name missing key `seed`: {err}"
        );

        // 2. Unknown key
        let unknown = format!("{BASE_TOML}unknown_key = 123\n");
        let err = Base::parse(&unknown).expect_err("unknown key must fail");
        assert!(
            err.contains("unknown_key"),
            "error message must name unknown key `unknown_key`: {err}"
        );

        // 3. Zero value
        let zero = BASE_TOML.replace("seed = 1", "seed = 0");
        let err = Base::parse(&zero).expect_err("zero value must fail");
        assert!(
            err.contains("seed"),
            "error message must name zero key `seed`: {err}"
        );

        // 4. String value
        let str_val = BASE_TOML.replace("seed = 1", "seed = \"not_an_int\"");
        let err = Base::parse(&str_val).expect_err("string value must fail");
        assert!(
            err.contains("seed"),
            "error message must name non-int key `seed`: {err}"
        );
    }

    fn tree_from_generated(generated: &Generated) -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        for (path, bytes) in &generated.files {
            if let Some((dir, _)) = path.rsplit_once('/') {
                tree.insert_dir(dir);
            }
            if path.starts_with("corpus/sources/public/") {
                let mut hasher = sha2::Sha256::new();
                hasher.update(bytes);
                let sha256 = fieldbook_core::hex::lower(&hasher.finalize());
                tree.insert_original(path, sha256, bytes.len() as u64);
            } else {
                tree.insert_bytes(path, bytes.clone());
            }
        }
        tree
    }

    #[test]
    fn generated_corpus_is_clean_under_strict() {
        let tiny_base = Base::parse(TINY_TOML).expect("parse tiny");
        let full_base = Base::parse(BASE_TOML).expect("parse base");
        let today = CivilDate::parse(TODAY_TEXT.trim()).expect("valid TODAY date");

        for (label, base, m) in [
            ("tiny 1x", &tiny_base, 1),
            ("tiny 3x", &tiny_base, 3),
            ("base 1x", &full_base, 1),
        ] {
            let generated = generate(base, m, TODAY_TEXT).expect("generation failed");
            let tree = tree_from_generated(&generated);
            let report = check_run(&Host::new(tree, today), &Options::new(true));

            assert!(
                report.diagnostics.is_empty(),
                "diagnostics found for {label} (count {}):\n{}",
                report.diagnostics.len(),
                report
                    .diagnostics
                    .iter()
                    .take(20)
                    .map(|d| format!("  {} {}:{}:{} {}", d.code, d.path, d.line, d.col, d.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let base = Base::parse(TINY_TOML).expect("parse tiny");
        let gen1 = generate(&base, 1, TODAY_TEXT).expect("gen1");
        let gen2 = generate(&base, 1, TODAY_TEXT).expect("gen2");

        assert_eq!(gen1.files, gen2.files);
        assert_eq!(gen1.counts, gen2.counts);
        assert_eq!(gen1.query, gen2.query);

        let mut diff_base = base.clone();
        diff_base.seed = base.seed + 100;
        let gen3 = generate(&diff_base, 1, TODAY_TEXT).expect("gen3");
        assert_ne!(gen1.files, gen3.files);
    }

    #[test]
    fn counts_scale_with_the_size() {
        let tiny_base = Base::parse(TINY_TOML).expect("parse tiny");
        let gen1 = generate(&tiny_base, 1, TODAY_TEXT).expect("gen 1x");
        let gen5 = generate(&tiny_base, 5, TODAY_TEXT).expect("gen 5x");

        assert_eq!(gen5.counts.areas, gen1.counts.areas);
        assert_eq!(gen5.counts.pages, gen1.counts.pages * 5);
        assert_eq!(gen5.counts.claims, gen1.counts.claims * 5);
        assert_eq!(gen5.counts.sources, gen1.counts.sources * 5);
        assert_eq!(gen5.counts.questions, gen1.counts.questions * 5);
        assert_eq!(gen5.counts.conflicts, gen1.counts.conflicts * 5);
        assert_eq!(gen5.counts.terms, gen1.counts.terms * 5);

        // Path uniqueness.
        let mut paths = BTreeSet::new();
        for (path, _) in &gen5.files {
            assert!(paths.insert(path.clone()), "duplicate file path: {path}");
        }

        // ID uniqueness from first column of registers (and (source_id, version) for sources).
        for reg_file in [
            "corpus/registers/claims.csv",
            "corpus/registers/questions.csv",
            "corpus/registers/conflicts.csv",
            "corpus/registers/glossary.csv",
        ] {
            let (_, bytes) = gen5
                .files
                .iter()
                .find(|(p, _)| p == reg_file)
                .expect("register file exists");
            let text = std::str::from_utf8(bytes).expect("utf8 register");
            let mut ids = BTreeSet::new();
            for (idx, line) in text.lines().enumerate() {
                if idx == 0 || line.trim().is_empty() {
                    continue;
                }
                let first_col = line.split(',').next().unwrap_or("");
                assert!(
                    ids.insert(first_col.to_owned()),
                    "duplicate ID in {reg_file}: {first_col}"
                );
            }
        }

        let (_, bytes) = gen5
            .files
            .iter()
            .find(|(p, _)| p == "corpus/registers/sources.csv")
            .expect("sources.csv exists");
        let text = std::str::from_utf8(bytes).expect("utf8 register");
        let mut vids = BTreeSet::new();
        for (idx, line) in text.lines().enumerate() {
            if idx == 0 || line.trim().is_empty() {
                continue;
            }
            let mut parts = line.split(',');
            let sid = parts.next().unwrap_or("");
            let ver = parts.next().unwrap_or("");
            let vid = format!("{sid}-v{ver}");
            assert!(
                vids.insert(vid.clone()),
                "duplicate version ID in sources.csv: {vid}"
            );
        }
    }

    #[test]
    fn question_capacity_is_checked() {
        let mut base = Base::parse(TINY_TOML).expect("parse tiny");
        base.questions = 999;
        assert!(validate(&base, 1).is_ok());

        base.questions = 1000;
        let err = validate(&base, 1).expect_err("1000 questions must fail");
        assert!(
            err.contains("questions"),
            "error message must name questions: {err}"
        );

        // Overflow multiplication.
        base.questions = u64::MAX;
        let err = validate(&base, 2).expect_err("overflow must return Err");
        assert!(
            err.contains("questions"),
            "error message must name questions: {err}"
        );

        // scale/base.toml at 20x validates and has 980 questions.
        let full_base = Base::parse(BASE_TOML).expect("parse base");
        let sizes_20x = validate(&full_base, 20).expect("20x must validate");
        assert_eq!(sizes_20x.questions, 980);
    }

    #[test]
    fn oversized_run_is_refused_before_anything_is_written() {
        let full_base = Base::parse(BASE_TOML).expect("parse base");
        let err = validate(&full_base, 21).expect_err("21x must fail validation");
        assert!(
            err.contains("questions"),
            "error message must name questions: {err}"
        );
        assert!(err.contains("1029") || err.contains("999"));
    }

    #[test]
    fn table_has_the_columns() {
        let row = Row {
            size: 1,
            units: Some(10),
            corpus_kb: 50,
            check_s: 0.12,
            check_path_s: 0.05,
            build_s: 0.34,
            build_stopped: false,
            search_ms: 12,
            index_kb: Some(25),
            rss_mb: Some(40),
        };
        let output = render_table("scale/base.toml", 1, &[row]);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() >= 3);
        assert_eq!(lines[0], "scale base=scale/base.toml seed=1");
        assert_eq!(
            lines[1],
            "size  units  corpus_kb  check_s  check_path_s  build_s  search_ms  index_kb  rss_mb"
        );
        let tokens: Vec<&str> = lines[2].split_whitespace().collect();
        assert_eq!(
            tokens,
            ["1x", "10", "50", "0.12", "0.05", "0.34", "12", "25", "40"]
        );
    }

    #[test]
    fn a_stopped_build_is_starred() {
        let row = Row {
            size: 20,
            units: None,
            corpus_kb: 500,
            check_s: 1.23,
            check_path_s: 0.45,
            build_s: 2.34,
            build_stopped: true,
            search_ms: 15,
            index_kb: None,
            rss_mb: Some(100),
        };
        let output = render_table("scale/base.toml", 1, &[row]);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() >= 4);
        let tokens: Vec<&str> = lines[2].split_whitespace().collect();
        assert_eq!(tokens[0], "20x");
        assert_eq!(tokens[1], "-");
        assert_eq!(tokens[5], "2.34*");
        assert_eq!(tokens[7], "-");
        assert_eq!(
            lines[3],
            "* build stopped by FB0605 (a data file over the cap); the time is to the stop."
        );
    }

    #[test]
    fn nfr_lines_say_within_or_over() {
        assert_eq!(
            nfr_line(1, 5000),
            Some("NFR-F-04 check 1x: 5.000 s, budget 5 s, within".to_string())
        );
        assert_eq!(
            nfr_line(1, 5001),
            Some("NFR-F-04 check 1x: 5.001 s, budget 5 s, over".to_string())
        );
        assert_eq!(
            nfr_line(20, 60000),
            Some("NFR-F-04 check 20x: 60.000 s, budget 60 s, within".to_string())
        );
        assert_eq!(
            nfr_line(20, 60001),
            Some("NFR-F-04 check 20x: 60.001 s, budget 60 s, over".to_string())
        );
        assert_eq!(nfr_line(5, 5000), None);
    }

    #[test]
    fn vmhwm_is_read_from_status() {
        assert_eq!(vmhwm_kb("VmHWM:     123456 kB"), Some(123456));
        assert_eq!(vmhwm_kb("other text without VmHWM line\n"), None);
        let multiline =
            "Name:\tfieldbook\nVmPeak:\t  200000 kB\nVmHWM:\t  123456 kB\nVmRSS:\t   50000 kB\n";
        assert_eq!(vmhwm_kb(multiline), Some(123456));
    }

    #[test]
    fn oversized_cli_run_creates_no_folder() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo root");
        let pid = std::process::id();
        let prefix = format!("fieldbook-scale-{pid}");
        let temp = std::env::temp_dir();

        let count_before = std::fs::read_dir(&temp)
            .expect("read temp")
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .count();

        let mut out = String::new();
        let code = run(root, &["--sizes", "21", "--bin", "/bin/false"], &mut out);
        assert_eq!(code, 2);

        let exit = command(root, &["--sizes", "21", "--bin", "/bin/false"]);
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::from(2)));

        let count_after = std::fs::read_dir(&temp)
            .expect("read temp")
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .count();
        assert_eq!(count_before, count_after);
    }

    #[test]
    fn cli_usage_errors() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo root");
        let mut out = String::new();

        assert_eq!(run(root, &["--unknown-flag"], &mut out), 2);
        assert_eq!(run(root, &["--sizes", "0"], &mut out), 2);
        assert_eq!(run(root, &["--sizes", "1,1"], &mut out), 2);
        assert_eq!(run(root, &["--sizes"], &mut out), 2);
        assert_eq!(run(root, &["--base"], &mut out), 2);
        assert_eq!(run(root, &["--bin"], &mut out), 2);
    }
}
