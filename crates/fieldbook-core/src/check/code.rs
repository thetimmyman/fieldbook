//! The diagnostic codes of the Check catalogue (`docs/reference/corpus-format.md#check-catalogue`).

use std::fmt;
use std::str::FromStr;

use super::diag::Severity;

/// The catalogue severity level of a [`Code`], determined from the first word of its severity text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Reported as an error regardless of `--strict`.
    Error,
    /// Reported as a warning regardless of `--strict`.
    Warning,
    /// Reported as a warning by default and as an error under `--strict`.
    Strict,
}

impl Level {
    /// Parse a [`Level`] from the first word of a catalogue severity string (`error`, `warning`, or `strict`).
    #[must_use]
    pub const fn from_severity_text(text: &str) -> Self {
        let bytes = text.as_bytes();
        if bytes.len() >= 5
            && bytes[0] == b'e'
            && bytes[1] == b'r'
            && bytes[2] == b'r'
            && bytes[3] == b'o'
            && bytes[4] == b'r'
        {
            Self::Error
        } else if bytes.len() >= 6
            && bytes[0] == b's'
            && bytes[1] == b't'
            && bytes[2] == b'r'
            && bytes[3] == b'i'
            && bytes[4] == b'c'
            && bytes[5] == b't'
        {
            Self::Strict
        } else {
            Self::Warning
        }
    }
}

/// Error returned when a string is not a known [`Code`] in the Check catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeParseError(String);

impl CodeParseError {
    /// The input string that failed to parse.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CodeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown check code `{}`", self.0)
    }
}

impl std::error::Error for CodeParseError {}

macro_rules! define_codes {
    ($(($variant:ident, $id:literal, $sev_text:literal, $help:literal)),+ $(,)?) => {
        /// Every diagnostic code of the Check catalogue (`FB0100` to `FB0955`).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Code {
            $(
                #[doc = concat!("`", $id, "` (`", $sev_text, "`): ", $help)]
                $variant,
            )+
        }

        impl Code {
            /// Every [`Code`] in catalogue order.
            pub const ALL: &'static [Self] = &[
                $(Self::$variant),+
            ];

            /// The canonical 6-character code string (such as `"FB0102"`).
            #[must_use]
            pub const fn id(self) -> &'static str {
                match self {
                    $(Self::$variant => $id,)+
                }
            }

            /// The exact severity cell text from the Check catalogue in `docs/reference/corpus-format.md`.
            #[must_use]
            pub const fn severity_text(self) -> &'static str {
                match self {
                    $(Self::$variant => $sev_text,)+
                }
            }

            /// The one-sentence remediation advice printed on the `help:` line of a diagnostic.
            #[must_use]
            pub const fn help(self) -> &'static str {
                match self {
                    $(Self::$variant => $help,)+
                }
            }

            /// The [`Level`] of this code (`Error`, `Warning`, or `Strict`), derived from the first word of [`Self::severity_text`].
            #[must_use]
            pub const fn level(self) -> Level {
                match self {
                    $(Self::$variant => Level::from_severity_text($sev_text),)+
                }
            }

            /// The effective [`Severity`] of this code given whether `--strict` is active.
            #[must_use]
            pub const fn severity(self, strict: bool) -> Severity {
                match self.level() {
                    Level::Error => Severity::Error,
                    Level::Warning => Severity::Warning,
                    Level::Strict => {
                        if strict {
                            Severity::Error
                        } else {
                            Severity::Warning
                        }
                    }
                }
            }

            /// Parse a [`Code`] from its exact `"FBnnnn"` string.
            ///
            /// # Errors
            ///
            /// Returns [`CodeParseError`] when `s` is not one of the codes in [`Self::ALL`].
            pub fn parse(s: &str) -> Result<Self, CodeParseError> {
                match s {
                    $($id => Ok(Self::$variant),)+
                    _ => Err(CodeParseError(s.to_owned())),
                }
            }
        }
    };
}

define_codes! {
    // 01: pages, corpus.toml and files
    (
        Fb0100,
        "FB0100",
        "error",
        "Fix corpus/corpus.toml (or store.toml in a company store) so it is valid TOML at a supported schema_version with well-formed areas and views."
    ),
    (
        Fb0101,
        "FB0101",
        "error",
        "Begin the page with balanced +++ TOML front-matter fences."
    ),
    (
        Fb0102,
        "FB0102",
        "error",
        "Fix the page front matter so every key and value matches the TOML schema."
    ),
    (
        Fb0103,
        "FB0103",
        "error",
        "Add the missing required front-matter field to the page."
    ),
    (
        Fb0104,
        "FB0104",
        "error",
        "Use a permitted vocabulary value and form for the front-matter field."
    ),
    (
        Fb0105,
        "FB0105",
        "error",
        "Give the page a unique valid page id that equals its file stem."
    ),
    (
        Fb0106,
        "FB0106",
        "error",
        "Set area to a declared area code that matches both the page id prefix and the area folder."
    ),
    (
        Fb0107,
        "FB0107",
        "error",
        "Set subarea to a declared subarea of the page's area that matches the folder name."
    ),
    (
        Fb0108,
        "FB0108",
        "error",
        "Set type to a page type supported by this schema version."
    ),
    (
        Fb0109,
        "FB0109",
        "error",
        "Remove the computed field from front matter; the tool computes it."
    ),
    (
        Fb0110,
        "FB0110",
        "error",
        "Reference only existing page ids in prereq, links, supersedes and superseded_by."
    ),
    (
        Fb0111,
        "FB0111",
        "error",
        "Remove the prerequisite cycle or point prereq only to pages at the same or an earlier stage."
    ),
    (
        Fb0112,
        "FB0112",
        "error",
        "Reference only term_id values that exist in corpus/registers/glossary.csv."
    ),
    (
        Fb0113,
        "FB0113",
        "error",
        "Reference only question_id and conflict_id values that exist in their registers."
    ),
    (
        Fb0114,
        "FB0114",
        "error",
        "Write a real ISO 8601 calendar date in the form the field requires."
    ),
    (
        Fb0115,
        "FB0115",
        "error",
        "Make page supersession reciprocal between distinct pages without loops."
    ),
    (
        Fb0116,
        "FB0116",
        "warning",
        "Set the page's as_of date to today or an earlier calendar date."
    ),
    (
        Fb0117,
        "FB0117",
        "warning",
        "Re-check the page against its claims and update its as_of date."
    ),
    (
        Fb0118,
        "FB0118",
        "strict",
        "Refresh the older cited claim or set the page's as_of date no later than its oldest cited claim."
    ),
    (
        Fb0120,
        "FB0120",
        "error",
        "Re-encode the file as valid UTF-8."
    ),
    (
        Fb0121,
        "FB0121",
        "error",
        "Move or remove the file or folder so the directory tree matches the Corpus layout."
    ),
    (
        Fb0122,
        "FB0122",
        "error",
        "Run fieldbook fmt to normalize line endings, Unicode NFC, trailing whitespace and register formatting."
    ),
    (
        Fb0123,
        "FB0123",
        "error",
        "Rename the path so it obeys the length, character, case and reserved-name rules."
    ),
    (
        Fb0124,
        "FB0124",
        "error",
        "Format corpus/changes.md with newest-first ## edition-NNN headings and - Added:, - Changed: or - Verified: bullets without dates."
    ),
    (
        Fb0125,
        "FB0125",
        "error",
        "Keep corpus/ASSISTANT.md (starting with rules_version = <integer>) and corpus/AGENTS.md within 40 lines."
    ),
    (
        Fb0126,
        "FB0126",
        "error",
        "Fix corpus/eval/regression.toml so every case has a unique id and existing expected unit ids."
    ),
    (
        Fb0127,
        "FB0127",
        "warning",
        "Check the regression case and the Corpus content; a person decides whether to change the case. Do not tune the ranking to pass it."
    ),
    (
        Fb0128,
        "FB0128",
        "error",
        "Check the critical regression case and the Corpus content; a person decides whether to change the case. Do not tune the ranking to pass it."
    ),
    (
        Fb0130,
        "FB0130",
        "error",
        "Use the exact ## part headings for the page's type in order, with no # heading and no heading deeper than ###."
    ),
    (
        Fb0131,
        "FB0131",
        "error",
        "Write content in the part or write the exact empty-part phrase Not established."
    ),
    (
        Fb0132,
        "FB0132",
        "error",
        "Leave ## Sources empty in the Markdown source; build fills it from the page's citations."
    ),
    (
        Fb0133,
        "FB0133",
        "error",
        "Start a non-empty Worked example part with the synthetic example label."
    ),
    (
        Fb0134,
        "FB0134",
        "error",
        "Include the required structural content for this page type."
    ),
    (
        Fb0135,
        "FB0135",
        "error",
        "Remove raw HTML, HTML comments, images and external links from the page body."
    ),
    (
        Fb0136,
        "FB0136",
        "error",
        "Point the relative Markdown link to an existing file in the Corpus."
    ),
    (
        Fb0137,
        "FB0137",
        "error",
        "Format ## Change note with newest-first - CLASS, YYYY-MM-DD: sentence. bullets ending with NEW_KNOWLEDGE."
    ),
    (
        Fb0138,
        "FB0138",
        "warning",
        "Trim the brief page body to at most 400 words."
    ),
    (
        Fb0139,
        "FB0139",
        "warning",
        "List the term in front-matter terms instead of restating its definition in bold prose."
    ),

    // 02: registers and sources
    (
        Fb0200,
        "FB0200",
        "error",
        "Ensure the register CSV exists and starts with the exact specification header columns in order."
    ),
    (
        Fb0201,
        "FB0201",
        "error",
        "Fix the CSV syntax so quotes balance, every row has the header column count, and no cell contains a line break."
    ),
    (
        Fb0202,
        "FB0202",
        "error",
        "Re-save the register file as CSV UTF-8."
    ),
    (
        Fb0203,
        "FB0203",
        "error",
        "Give every register row a well-formed, unique primary key."
    ),
    (
        Fb0204,
        "FB0204",
        "error",
        "Put a value in the cell that matches its column type, vocabulary, form and length limit."
    ),
    (
        Fb0205,
        "FB0205",
        "error",
        "Fill the required register cell."
    ),
    (
        Fb0206,
        "FB0206",
        "error",
        "Update the row's cells so they satisfy the register's cross-column rules."
    ),
    (
        Fb0207,
        "FB0207",
        "error",
        "Point the cell to an existing source version, claim, question, conflict, term or area."
    ),
    (
        Fb0208,
        "FB0208",
        "error",
        "Make register supersession links reciprocal between distinct rows without loops."
    ),
    (
        Fb0209,
        "FB0209",
        "error",
        "Satisfy every promotion condition and role requirement before setting primary_source_supported."
    ),
    (
        Fb0210,
        "FB0210",
        "error",
        "Use a public-Corpus vocabulary value; company-store-only values belong only in a company store."
    ),
    (
        Fb0211,
        "FB0211",
        "error",
        "Ensure every non-metadata-only source version has its matching byte-exact original file and no extra file sits in sources/."
    ),
    (
        Fb0212,
        "FB0212",
        "error",
        "Re-run cargo xtask extract or fix extraction.toml so the text files, hashes, counts and locator anchors match."
    ),
    (
        Fb0213,
        "FB0213",
        "error",
        "Move the verified claim to an active source version or reset its verification status."
    ),
    (
        Fb0214,
        "FB0214",
        "error",
        "Give each glossary term a label and aliases that are unique across glossary.csv, ignoring case."
    ),
    (
        Fb0215,
        "FB0215",
        "error",
        "Set origin_system, origin_stream and origin_key together from declared systems.toml entries and keep doc_kind and series valid for the store."
    ),
    (
        Fb0216,
        "FB0216",
        "error",
        "Link each new revision to the version it replaces with a non-decreasing as_of date and a unique origin_key per stream."
    ),
    (
        Fb0220,
        "FB0220",
        "warning",
        "Set the claim's as_of date to today or an earlier date."
    ),
    (
        Fb0221,
        "FB0221",
        "warning",
        "Re-verify the claim against its source and update verified_on or as_of."
    ),
    (
        Fb0222,
        "FB0222",
        "warning",
        "Move the claim to the superseding source version and re-verify it, or supersede the claim."
    ),
    (
        Fb0223,
        "FB0223",
        "warning",
        "Check the consequential claim directly against the original document because its extraction is metadata-only, thin or supplied."
    ),
    (
        Fb0230,
        "FB0230",
        "strict",
        "Fill owner_role, blocks, why_it_matters, default_assumption and work_can_continue on every day_one question."
    ),
    (
        Fb0240,
        "FB0240",
        "error",
        "Fix the company store's types.toml so every type revision, column kind, reference and record file matches the schema."
    ),
    (
        Fb0241,
        "FB0241",
        "error",
        "Fix the record file so version numbers, supersession chains, types, keys and columns follow the type definition."
    ),

    // 03: citations
    (
        Fb0300,
        "FB0300",
        "error",
        "Write the citation with balanced brackets and a valid identifier after @."
    ),
    (
        Fb0301,
        "FB0301",
        "error",
        "Rewrite the citation in canonical [@ID] or [@S-nnnn-vN, <locator>] form."
    ),
    (
        Fb0302,
        "FB0302",
        "error",
        "Separate adjacent citations with a space or punctuation, or combine keys in one bracket pair."
    ),
    (
        Fb0303,
        "FB0303",
        "error",
        "Cite an identifier that exists in the registers."
    ),
    (
        Fb0304,
        "FB0304",
        "error",
        "Use a page number within page_count or a section label declared in the version's [locators] table."
    ),
    (
        Fb0305,
        "FB0305",
        "error",
        "Remove the locator from the claim, question or conflict citation; only source-version citations take a locator."
    ),
    (
        Fb0306,
        "FB0306",
        "error",
        "Give the consequential claim a fine locator (not no_fine_locator)."
    ),
    (
        Fb0307,
        "FB0307",
        "error",
        "Add a verbatim quote of at most 300 characters to the consequential claim."
    ),
    (
        Fb0308,
        "FB0308",
        "warning, error from Day 30",
        "Copy the exact wording from the extracted text at the claim's locator."
    ),
    (
        Fb0309,
        "FB0309",
        "warning, error from Day 30",
        "Add a [@CL-...] claim citation in the same sentence as the figure, or move the figure into an exempt block."
    ),
    (
        Fb0310,
        "FB0310",
        "warning",
        "Update the citation to point to the current active source version or replacement claim."
    ),
    (
        Fb0311,
        "FB0311",
        "error",
        "Point the rec, sheet or field locator to an existing row, sheet range or non-empty record column."
    ),
    (
        Fb0320,
        "FB0320",
        "strict",
        "Verify every consequential claim cited by the stable page to primary_source_supported, or keep the page below stable."
    ),
    (
        Fb0380,
        "FB0380",
        "error, from `r2-records`",
        "Supply the computed field's inputs and matching stored run, or provide a non-empty rationale for judgment evidence."
    ),
    (
        Fb0381,
        "FB0381",
        "error, from `r2-records`",
        "Provide a single resolved evidence pointer whose value, unit and quotation match the quoted numeric field."
    ),
    (
        Fb0382,
        "FB0382",
        "warning, from `r2-records`",
        "Re-run the computed record against active, current inputs and write a new record version."
    ),

    // 04: scans, labels and stores
    (
        Fb0400,
        "FB0400",
        "error",
        "Remove the controlled or proprietary marking from the public Corpus file, or record a narrow public-document allowance in corpus/scan-allow.toml."
    ),
    (
        Fb0401,
        "FB0401",
        "error",
        "Remove the email address, phone number, private IP, internal host name, home path or tracker key from the Corpus file."
    ),
    (
        Fb0402,
        "FB0402",
        "error",
        "Replace the prohibited compliance or corroboration phrase with specific factual wording that names the sources."
    ),
    (
        Fb0403,
        "FB0403",
        "error",
        "Remove the leading spreadsheet formula character (=, +, -, @, TAB or CR) from the register text cell."
    ),
    (
        Fb0404,
        "FB0404",
        "error",
        "Fix corpus/scan-allow.toml so every [[allow]] entry has a valid sha256, match and non-empty reason without duplicates."
    ),
    (
        Fb0405,
        "FB0405",
        "warning",
        "Remove or update the stale [[allow]] entry in corpus/scan-allow.toml whose sha256 and match no longer occur."
    ),
    (
        Fb0410,
        "FB0410",
        "error",
        "Keep handling set to public in the repository Corpus, and pass --confirm-public to home add or new only when the input is public."
    ),
    (
        Fb0411,
        "FB0411",
        "warning",
        "Add the label id to the company policy's label table or correct the item's handling labels."
    ),
    (
        Fb0412,
        "FB0412",
        "error",
        "Send the item only to a store whose holds list includes every label the item carries."
    ),
    (
        Fb0413,
        "FB0413",
        "error",
        "Fix the company policy file so its syntax, label ids and revision number are valid and non-decreasing."
    ),
    (
        Fb0414,
        "FB0414",
        "error",
        "Load the policy file from its administrator-managed location outside the data root, the Corpus and any git work tree."
    ),
    (
        Fb0415,
        "FB0415",
        "error",
        "Keep stores, state folders, drop folders and non-public records outside git work trees, sync roots, reparse points and disallowed network paths, and keep .fieldbook/ ignored at home."
    ),
    (
        Fb0416,
        "FB0416",
        "error",
        "Remove the company identifier or company area code from the public repository Corpus."
    ),
    (
        Fb0417,
        "FB0417",
        "error",
        "Use a company-store identifier shape (such as SX- or CL-XAA-) for rows defined in the company store."
    ),
    (
        Fb0418,
        "FB0418",
        "error",
        "Restrict write permissions on the policy file and its parent folder to administrators."
    ),
    (
        Fb0419,
        "FB0419",
        "error",
        "Omit folder-override flags on the laptop preset so Fieldbook uses the fixed %LOCALAPPDATA%\\Fieldbook folders."
    ),
    (
        Fb0420,
        "FB0420",
        "error",
        "Set FILE_ATTRIBUTE_NOT_CONTENT_INDEXED (attrib +I) on the store folder."
    ),

    // 05: calculation
    (
        Fb0500,
        "FB0500",
        "error",
        "Write the decimal in strict canonical grammar (optional -, digits, optional .digits) with a permitted rounding scale."
    ),
    (
        Fb0501,
        "FB0501",
        "error",
        "Keep calculation inputs and intermediate results within the representable range of the calculation type."
    ),
    (
        Fb0502,
        "FB0502",
        "error",
        "Combine only values that share the same currency or compatible unit family."
    ),
    (
        Fb0503,
        "FB0503",
        "error",
        "Fix the calculation or the case expectation so the computed result equals the expected value."
    ),
    (
        Fb0504,
        "FB0504",
        "error",
        "Name a registered calculation function from CALC_FNS."
    ),

    // 06: site and edition
    (
        Fb0600,
        "FB0600",
        "error",
        "Fix the Corpus errors reported by fieldbook check before running fieldbook build."
    ),
    (
        Fb0601,
        "FB0601",
        "error",
        "Remove disallowed or quarantined files from the edition tree and ensure every required edition file is present."
    ),
    (
        Fb0602,
        "FB0602",
        "error",
        "Shorten the edition relative path to at most 150 characters."
    ),
    (
        Fb0603,
        "FB0603",
        "error",
        "Fix the template or link target so every link in the built site resolves to an emitted file and anchor."
    ),
    (
        Fb0604,
        "FB0604",
        "error",
        "Remove external hrefs, inline styles, inline event handlers and non-asset scripts, and emit the exact CSP_STATIC policy."
    ),
    (
        Fb0605,
        "FB0605",
        "error",
        "Trim the script or data file so it stays within its byte budget."
    ),
    (
        Fb0606,
        "FB0606",
        "error",
        "Remove the non-deterministic ordering or timestamp so two builds of the same commit produce identical bytes."
    ),
    (
        Fb0607,
        "FB0607",
        "warning on Linux, error in the `windows` job",
        "Shorten the brief page so its printed layout fits on a single Letter page."
    ),
    (
        Fb0608,
        "FB0608",
        "error",
        "Rebuild the edition or restore the modified file so SHA256SUMS, edition.toml and extraction hashes match."
    ),

    // 07: Handoff
    (
        Fb0700,
        "FB0700",
        "error",
        "Remove the controlled or proprietary marking from the question before building a context pack."
    ),
    (
        Fb0701,
        "FB0701",
        "error",
        "Replace or remove the broken citation so every cited id exists in the Corpus."
    ),
    (
        Fb0702,
        "FB0702",
        "error",
        "Fix the citation locator so it resolves in the cited source version's extraction."
    ),
    (
        Fb0703,
        "FB0703",
        "warning",
        "Update the answer to cite the current active source version or replacement claim."
    ),
    (
        Fb0704,
        "FB0704",
        "warning",
        "Support the statement with a primary_source_supported claim on a Tier A or Tier B source instead of weak evidence."
    ),
    (
        Fb0705,
        "FB0705",
        "warning",
        "Add a citation to the sentence or use the exact abstention phrase Not established in the library."
    ),
    (
        Fb0706,
        "FB0706",
        "warning",
        "Rewrite the citation in canonical [@ID] form."
    ),
    (
        Fb0707,
        "FB0707",
        "error",
        "Do not use the answer; obtain one whose citations resolve and verify against the context pack."
    ),
    (
        Fb0708,
        "FB0708",
        "error",
        "Remove markings, identity shapes, prohibited claim language and formula triggers before saving a public note."
    ),
    (
        Fb0709,
        "FB0709",
        "error",
        "Provide complete note front matter with question, reviewed, a 64-hex context_fingerprint and origin = \"ai_drafted\"."
    ),

    // 08: audit, backup and pipeline
    (
        Fb0800,
        "FB0800",
        "error",
        "Run fieldbook audit verify --repair to quarantine the broken journal chain and start a recovery chain."
    ),
    (
        Fb0801,
        "FB0801",
        "error",
        "Ensure every journal event uses only allowed event kinds, closed-vocabulary fields and required actor, role, store and via keys."
    ),
    (
        Fb0802,
        "FB0802",
        "error",
        "Restore the truncated or mismatched journal from a verified backup or run audit verify --repair."
    ),
    (
        Fb0803,
        "FB0803",
        "warning",
        "Inspect the unfinished transaction or model call in the audit journal and re-run the interrupted command if needed."
    ),
    (
        Fb0804,
        "FB0804",
        "warning",
        "Keep the quarantined journal.broken-<seq> file for the audit record and verify the new chain."
    ),
    (
        Fb0805,
        "FB0805",
        "warning",
        "Check the system clock; journal event timestamps should not move backwards."
    ),
    (
        Fb0806,
        "FB0806",
        "error",
        "Configure a writable [audit] sink folder (or pass --to) where no existing export file would be overwritten with different bytes."
    ),
    (
        Fb0807,
        "FB0807",
        "error",
        "Run the command as an actor and role that the active Authz policy permits for this store and operation."
    ),
    (
        Fb0808,
        "FB0808",
        "warning",
        "Restore the missing store folder or verify that its removal was intentional."
    ),
    (
        Fb0810,
        "FB0810",
        "error",
        "Do not restore from the corrupted snapshot; take a fresh backup or select an earlier verified snapshot."
    ),
    (
        Fb0811,
        "FB0811",
        "error",
        "Take a fresh backup so backup.toml and SHA256SUMS match the files in the snapshot."
    ),
    (
        Fb0812,
        "FB0812",
        "error",
        "Choose a writable backup target folder that lies outside every folder being backed up."
    ),
    (
        Fb0813,
        "FB0813",
        "warning",
        "Declare kind = \"synced\" on the backup target in settings.toml or move the backup target to a local folder."
    ),
    (
        Fb0814,
        "FB0814",
        "warning",
        "Run fieldbook backup and fieldbook backup verify to create a current verified backup."
    ),
    (
        Fb0815,
        "FB0815",
        "error",
        "Pass an empty destination folder to fieldbook restore and choose a snapshot that passes backup verify."
    ),
    (
        Fb0816,
        "FB0816",
        "error",
        "Investigate the restore drill mismatch in the snapshot's check, journal or fingerprint before relying on the backup."
    ),
    (
        Fb0820,
        "FB0820",
        "error",
        "Reject the altered proposal and re-propose the file from a clean copy."
    ),
    (
        Fb0821,
        "FB0821",
        "error",
        "Remove the executable or active-content file; only allowed document and table types may be added as originals."
    ),
    (
        Fb0822,
        "FB0822",
        "error",
        "Run the command interactively at a console as an authorized role or group and supply the required confirmation."
    ),
    (
        Fb0823,
        "FB0823",
        "warning",
        "Review and fill the dropped field by hand before accepting the proposal."
    ),
    (
        Fb0825,
        "FB0825",
        "warning",
        "Extract or supply text for the proposed source before relying on its quotes."
    ),
    (
        Fb0826,
        "FB0826",
        "error",
        "Fix corpus/pipeline/rules.toml so every rule has a unique id, valid vocabulary values, and an example that matches its expect table."
    ),
    (
        Fb0830,
        "FB0830",
        "error",
        "Fix systems.toml and the sync arguments so every system, stream, versioning mode and record type is valid."
    ),
    (
        Fb0831,
        "FB0831",
        "warning",
        "Inspect the skipped older, duplicate or unmatched item in the sync report if you expected it to be proposed."
    ),
    (
        Fb0832,
        "FB0832",
        "warning",
        "Allow sync to re-list the stream from the start and replace the unreadable or future cursor."
    ),
    (
        Fb0833,
        "FB0833",
        "error",
        "Remove symbolic links and relative path escapes so every pulled item stays inside its stream folder."
    ),
    (
        Fb0834,
        "FB0834",
        "error",
        "Fix the Pull adapter so it never mutates its source folder and lists items in a deterministic order."
    ),
    (
        Fb0835,
        "FB0835",
        "error",
        "Place the stream's drop folder inside the root directory of the policy [[source]] row that labels it."
    ),
    (
        Fb0836,
        "FB0836",
        "error",
        "Run fieldbook init --overlay --adopt to retire the old writer or assign a new writer before issuing ids in a restored store."
    ),

    // 09: model, decide, index, output, MCP and chat
    (
        Fb0900,
        "FB0900",
        "error",
        "Fix settings.toml so [model], [search], [mcp] and [chat] contain only known keys, registered adapters and no secret values."
    ),
    (
        Fb0901,
        "FB0901",
        "error",
        "Narrow the request scope or use a destination whose policy row and settings allow every label the request carries."
    ),
    (
        Fb0902,
        "FB0902",
        "error",
        "Remove the controlled or proprietary marking from the outbound prompt or label the input appropriately."
    ),
    (
        Fb0903,
        "FB0903",
        "error",
        "Restore write access to the state journal folder before running the command again."
    ),
    (
        Fb0904,
        "FB0904",
        "error",
        "Restore write access to the state journal folder and re-run the request; the unrecorded reply was discarded."
    ),
    (
        Fb0905,
        "FB0905",
        "error",
        "Set the endpoint's key_env environment variable to a valid API credential."
    ),
    (
        Fb0906,
        "FB0906",
        "error",
        "Check the endpoint base_url, model name and request parameters in settings.toml; redirects are not followed."
    ),
    (
        Fb0907,
        "FB0907",
        "error",
        "Narrow the --scope or lower the pack budget so the prompt stays within max_prompt_chars."
    ),
    (
        Fb0908,
        "FB0908",
        "error",
        "Check network connectivity to the approved endpoint or use --via handoff while the endpoint is unavailable."
    ),
    (
        Fb0909,
        "FB0909",
        "error",
        "Re-run the command or increase max_out_tokens so the model returns a complete, schema-valid reply."
    ),
    (
        Fb0910,
        "FB0910",
        "error",
        "Install the corporate root CA in the machine trust store or fix the endpoint's TLS certificate."
    ),
    (
        Fb0911,
        "FB0911",
        "error",
        "Run the AI build on a 64-bit CPU with the required instruction set extensions, or use --via handoff."
    ),
    (
        Fb0912,
        "FB0912",
        "error",
        "Use --via handoff on the silent build, or run an AI build compiled with the required net or bridge feature."
    ),
    (
        Fb0913,
        "FB0913",
        "error",
        "Set base_url to an https:// endpoint or a http://127.0.0.1 loopback address."
    ),
    (
        Fb0914,
        "FB0914",
        "error",
        "Export a non-empty API key in the environment variable named by key_env before running the command."
    ),
    (
        Fb0915,
        "FB0915",
        "error",
        "Start a new run or raise max_calls_per_run in settings.toml if more calls are needed."
    ),
    (
        Fb0916,
        "FB0916",
        "error",
        "Pass an unused H-nnnn handoff ticket and a non-empty reply file to --resume."
    ),
    (
        Fb0917,
        "FB0917",
        "error",
        "Reject the proposal; its provenance event is missing from or does not match the audit journal."
    ),
    (
        Fb0918,
        "FB0918",
        "error",
        "Use a destination that has an active, non-expired approval row in the company policy with a matching URL and client."
    ),
    (
        Fb0925,
        "FB0925",
        "error",
        "Run fieldbook audit export to ship recent journal events before making a non-public model call."
    ),
    (
        Fb0919,
        "FB0919",
        "error",
        "Lower max_out_tokens or raise max_reply_bytes in settings.toml so the reply fits within the cap."
    ),
    (
        Fb0920,
        "FB0920",
        "error",
        "Fix the decision spec TOML so it defines a question and at least two valid options."
    ),
    (
        Fb0921,
        "FB0921",
        "error",
        "Re-run decide or use a model that selects one of the declared option ids or unknown."
    ),
    (
        Fb0922,
        "FB0922",
        "error",
        "Re-run decide so the model returns an integer confidence between 0 and 1000."
    ),
    (
        Fb0923,
        "FB0923",
        "warning",
        "Inspect the decision report; citations outside the context pool or with unmatched quotes were dropped."
    ),
    (
        Fb0924,
        "FB0924",
        "warning",
        "Supply better-sourced context units; the decision pick was forced to unknown because no cited evidence survived."
    ),
    (
        Fb0930,
        "FB0930",
        "warning",
        "Rebuild the search index so the index adapter returns only unit ids present in the Corpus."
    ),
    (
        Fb0931,
        "FB0931",
        "error",
        "Fix the Output adapter so two renders of the same input produce identical bytes."
    ),
    (
        Fb0940,
        "FB0940",
        "error",
        "Send a well-formed newline-delimited JSON-RPC 2.0 request to fieldbook mcp."
    ),
    (
        Fb0941,
        "FB0941",
        "error",
        "Call one of the read-only MCP tools: search, get_unit, context, cite_check or open_questions."
    ),
    (
        Fb0942,
        "FB0942",
        "error",
        "Request only units whose labels are permitted for the mcp destination."
    ),
    (
        Fb0943,
        "FB0943",
        "error",
        "Pass valid JSON arguments matching the MCP tool's input schema."
    ),
    (
        Fb0944,
        "FB0944",
        "error",
        "Restore write access to the audit journal before calling fieldbook mcp tools."
    ),
    (
        Fb0950,
        "FB0950",
        "error",
        "Restart fieldbook chat in a terminal or use the page's Copy context pack Handoff controls."
    ),
    (
        Fb0951,
        "FB0951",
        "error",
        "Open the exact http://127.0.0.1:<port>/#t=<token> URL printed by fieldbook chat in a local browser tab."
    ),
    (
        Fb0952,
        "FB0952",
        "error",
        "Bind the chat Bridge only to 127.0.0.1."
    ),
    (
        Fb0953,
        "FB0953",
        "warning",
        "Run fieldbook chat again to start a new session, or raise [chat] idle_minutes in settings.toml."
    ),
    (
        Fb0954,
        "FB0954",
        "error",
        "Use the existing fieldbook chat session or stop it before starting another on the same state folder."
    ),
    (
        Fb0955,
        "FB0955",
        "error",
        "Send a request body of at most 256 KiB that completes within 5 seconds."
    ),
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl FromStr for Code {
    type Err = CodeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}
