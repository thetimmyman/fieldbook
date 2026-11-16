//! The `status` verb: what needs work, and optionally the desk pages (`AD-17`, `status-command`).
//!
//! **Responsibility.** Read the Corpus, `docs/maintenance.csv` and the state lines, hand them
//! to [`fieldbook_core::status`], print the report, and with `--page` write the desk pages.
//! Only `check` and `status` read today's date.
//!
//! **Rules.** Without `--page` it changes no file. It never calls a model.

use fieldbook_core::check::{self, Host};
use fieldbook_core::status::{self, parse_maintenance};

use crate::World;
use crate::cli::{EXIT_FINDINGS, EXIT_IO, EXIT_OK, StatusArgs};
use crate::{Outcome, pages};

/// The line printed when `settings.toml` has no role.
pub const VERIFY_OFF: &str = "verify is off until a role is set";

/// The line printed when no verified backup can be shown.
pub const NO_BACKUP_LINE: &str = "Last verified backup: none yet";

/// Runs `fieldbook status`.
#[must_use]
pub fn run(args: &StatusArgs, world: &dyn World) -> Outcome {
    let policy = world.effective_policy();
    run_with_policy(args, world, &policy)
}

/// Runs status using the operation's effective policy.
#[must_use]
pub fn run_with_policy(
    args: &StatusArgs,
    world: &dyn World,
    policy: &fieldbook_core::policy::EffectivePolicy,
) -> Outcome {
    let today = args.today.unwrap_or_else(|| world.today());
    let tree = match world.read_tree() {
        Ok(tree) => tree,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };
    let (model, findings) = check::load(&Host::new(tree, today), &check::Options::new(false));
    let load_errors: Vec<String> = findings
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.code,
                check::Code::Fb0100
                    | check::Code::Fb0114
                    | check::Code::Fb0200
                    | check::Code::Fb0201
                    | check::Code::Fb0202
                    | check::Code::Fb0203
                    | check::Code::Fb0204
                    | check::Code::Fb0205
                    | check::Code::Fb0206
            )
        })
        .map(check::Diagnostic::render_text)
        .collect();
    if !load_errors.is_empty() {
        return Outcome {
            stdout: String::new(),
            stderr: format!("{}\n", load_errors.join("\n")),
            code: EXIT_FINDINGS,
        };
    }

    let minutes_text = world.read_maintenance().unwrap_or_default();
    let minutes = if minutes_text.trim().is_empty() {
        None
    } else {
        match parse_maintenance(&minutes_text) {
            Ok(rows) => Some(rows),
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!(
                        "error: docs/maintenance.csv:{}: {}\n",
                        err.line, err.message
                    ),
                    code: EXIT_IO,
                };
            }
        }
    };

    let summary = world.read_state_summary(today).unwrap_or_default();
    let report = status::report(&model, today, minutes.as_deref(), &summary);
    let mut stdout = report.render();
    let mut code = EXIT_OK;

    if args.page {
        match pages::write_desk_pages(world, policy, today) {
            Ok(desk) => {
                for line in desk.lines {
                    stdout.push_str(&line);
                    stdout.push('\n');
                }
                if desk.source_failed {
                    code = EXIT_IO;
                }
            }
            Err(err) => {
                return Outcome {
                    stdout,
                    stderr: err,
                    code: EXIT_IO,
                };
            }
        }
    }

    Outcome {
        stdout,
        stderr: String::new(),
        code,
    }
}
