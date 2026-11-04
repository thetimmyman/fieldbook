//! [`Severity`], [`Diagnostic`] and [`Report`] (`AD-17`).

use std::fmt;

use super::code::Code;

/// Effective diagnostic severity (`error` or `warning`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// An error that makes `check` exit 1.
    Error,
    /// A warning that is reported without changing the exit code.
    Warning,
}

impl Severity {
    /// The lowercase word used in rendered text and JSON (`"error"` or `"warning"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One diagnostic finding raised by a check pass or verb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The catalogue [`Code`].
    pub code: Code,
    /// Effective [`Severity`] (`code.severity(strict)`).
    pub severity: Severity,
    /// Path relative to the repository root, `/`-separated.
    pub path: String,
    /// 1-based line number (1 for a finding about a whole file).
    pub line: usize,
    /// 1-based code-point column number (1 for a finding about a whole file).
    pub col: usize,
    /// Specific diagnostic message.
    pub message: String,
    /// Remediation help line (`code.help()`).
    pub help: &'static str,
}

impl Diagnostic {
    /// Construct a [`Diagnostic`], taking `severity` from `code.severity(strict)` and `help` from `code.help()`.
    #[must_use]
    pub fn new(
        code: Code,
        strict: bool,
        path: impl Into<String>,
        line: usize,
        col: usize,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: code.severity(strict),
            path: path.into(),
            line,
            col,
            message: message.into(),
            help: code.help(),
        }
    }

    /// Return the 1-based code-point column of `byte_offset` within `line_text`.
    ///
    /// Columns in diagnostics count Unicode scalar values (code points), starting at 1.
    #[must_use]
    pub fn col_at_byte(line_text: &str, byte_offset: usize) -> usize {
        let prefix = line_text.get(..byte_offset).unwrap_or_else(|| {
            let mut end = byte_offset.min(line_text.len());
            while end > 0 && !line_text.is_char_boundary(end) {
                end -= 1;
            }
            line_text.get(..end).unwrap_or("")
        });
        prefix.chars().count() + 1
    }

    /// Render this diagnostic in the two-line text form of `AD-17` (without trailing newline):
    /// `path:line:col: severity[CODE]: message\nhelp: text`.
    #[must_use]
    pub fn render_text(&self) -> String {
        format!(
            "{}:{}:{}: {}[{}]: {}\nhelp: {}",
            self.path,
            self.line,
            self.col,
            self.severity.as_str(),
            self.code.id(),
            self.message,
            self.help
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}[{}]: {}\nhelp: {}",
            self.path,
            self.line,
            self.col,
            self.severity.as_str(),
            self.code.id(),
            self.message,
            self.help
        )
    }
}

/// The collected diagnostics and notes of a `check` run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Diagnostics sorted by `(path, line, col, code)`.
    pub diagnostics: Vec<Diagnostic>,
    /// Plain note lines for facts that are not findings (such as a skipped check).
    pub notes: Vec<String>,
}

impl Report {
    /// Create an empty [`Report`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a [`Report`] from diagnostics and notes, sorting the diagnostics by `(path, line, col, code)`.
    #[must_use]
    pub fn from_parts(mut diagnostics: Vec<Diagnostic>, notes: Vec<String>) -> Self {
        Self::sort_slice(&mut diagnostics);
        Self { diagnostics, notes }
    }

    /// Add a [`Diagnostic`] to this report and keep `diagnostics` sorted by `(path, line, col, code)`.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
        Self::sort_slice(&mut self.diagnostics);
    }

    /// Add a plain note line to this report.
    pub fn push_note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    /// Sort `diagnostics` in place by `(path, line, col, code)`.
    pub fn sort(&mut self) {
        Self::sort_slice(&mut self.diagnostics);
    }

    fn sort_slice(diagnostics: &mut [Diagnostic]) {
        diagnostics.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.line.cmp(&b.line))
                .then_with(|| a.col.cmp(&b.col))
                .then_with(|| a.code.id().cmp(b.code.id()))
                .then_with(|| a.message.cmp(&b.message))
        });
    }

    /// The number of diagnostics with [`Severity::Error`].
    #[must_use]
    pub fn errors(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// The number of diagnostics with [`Severity::Warning`].
    #[must_use]
    pub fn warnings(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// Alias for [`Self::errors`].
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.errors()
    }

    /// Alias for [`Self::warnings`].
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.warnings()
    }

    /// True when the report has zero errors.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.errors() == 0
    }

    /// Render the full report in the text form of `AD-17`:
    /// diagnostics in sorted order separated by one blank line, then each note as `note: text`,
    /// and a final summary line `N errors, M warnings` (`clean` when both are zero).
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut sorted: Vec<&Diagnostic> = self.diagnostics.iter().collect();
        sorted.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.line.cmp(&b.line))
                .then_with(|| a.col.cmp(&b.col))
                .then_with(|| a.code.id().cmp(b.code.id()))
                .then_with(|| a.message.cmp(&b.message))
        });

        let mut out = String::new();
        for (idx, diag) in sorted.iter().enumerate() {
            if idx > 0 {
                out.push_str("\n\n");
            }
            out.push_str(&diag.render_text());
        }

        if !sorted.is_empty() {
            out.push('\n');
        }

        for note in &self.notes {
            out.push_str("note: ");
            out.push_str(note);
            out.push('\n');
        }

        let errs = self.errors();
        let warns = self.warnings();
        if errs == 0 && warns == 0 {
            out.push_str("clean\n");
        } else {
            out.push_str(&format!("{errs} errors, {warns} warnings\n"));
        }
        out
    }
}
