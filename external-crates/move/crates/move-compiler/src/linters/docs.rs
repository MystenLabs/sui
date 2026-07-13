// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Lookups and rendering for `move lint --explain <CODE>` and `--list`. The per-lint prose lives
//! in `diagnostics/explanations/<Category>_<CodeName>.md` files, attached to each lint's
//! [`DiagnosticInfo`] at compile time (see the `lints!`/`sui_lints!` macros); this module only
//! resolves user queries against the lint tables and wraps the file contents with the few lines
//! that would drift if hand-written -- the positional `Wxxyyy` code and the suppression
//! attribute.

use crate::{
    diagnostics::codes::DiagnosticInfo,
    linters::{LinterDiagnosticCategory, MOVE_LINTS},
    sui_mode::linters::SUI_LINTS,
};
use std::{collections::BTreeMap, fmt, sync::LazyLock};

type Lint = (&'static str, DiagnosticInfo);

/// Every lint as `(filter name, info)`, Move lints followed by Sui lints, in code order.
fn all_lints() -> impl Iterator<Item = &'static Lint> {
    MOVE_LINTS.iter().chain(SUI_LINTS.iter())
}

static LINTS_BY_NAME: LazyLock<BTreeMap<&'static str, &'static Lint>> =
    LazyLock::new(|| all_lints().map(|lint| (lint.0, lint)).collect());

static LINTS_BY_ID: LazyLock<BTreeMap<(u8, u8), &'static Lint>> = LazyLock::new(|| {
    all_lints()
        .map(|lint| ((lint.1.category(), lint.1.code()), lint))
        .collect()
});

/// The `--explain` output for a single lint: the explanation file printed verbatim, between a
/// generated `name (Wxxyyy): message` header and a generated suppression footer.
pub struct LintExplanation {
    name: &'static str,
    info: &'static DiagnosticInfo,
}

/// The rendered diagnostic code without the `Lint ` prefix, e.g. `W99000`. Lints are always
/// warnings, so the severity prefix is always `W`.
fn rendered_code(info: &DiagnosticInfo) -> String {
    format!("W{:02}{:03}", info.category(), info.code())
}

/// Look up a lint from raw `--explain <query>` user input: either a filter name (`share_owned`)
/// or a rendered diagnostic code (`W99000`). A lint without an explanation file is treated as
/// undocumented.
pub fn find_lint(query: &str) -> Option<LintExplanation> {
    let q = query.trim();
    let (name, info) = LINTS_BY_NAME.get(q).copied().or_else(|| {
        let (category, code) = parse_rendered_code(q)?;
        LINTS_BY_ID.get(&(category, code)).copied()
    })?;
    info.explanation()?;
    Some(LintExplanation { name, info })
}

/// Parse a rendered `Wxxyyy` code back into its `(category, code)` id. Accepts the forms a user is
/// likely to paste from diagnostic output: `W99000`, `w99000`, bare `99000`, or `Lint W99000` —
/// and the `E`-prefixed equivalents, since `#[deny(lint(...))]` and `--warnings-are-errors`
/// escalate lints to `error[Lint Exxyyy]`.
fn parse_rendered_code(s: &str) -> Option<(u8, u8)> {
    let s = s.strip_prefix("Lint ").unwrap_or(s).trim();
    let digits = s.strip_prefix(['W', 'w', 'E', 'e']).unwrap_or(s);
    if digits.len() != 5 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((digits[..2].parse().ok()?, digits[2..].parse().ok()?))
}

/// The filter name of the lint with the given diagnostic `(category, code)`, if it has an
/// explanation to point at. Used to attach the `--explain` hint to emitted lint diagnostics.
pub fn explained_lint_name(category: u8, code: u8) -> Option<&'static str> {
    let (name, info) = LINTS_BY_ID.get(&(category, code))?;
    info.explanation()?;
    Some(name)
}

impl fmt::Display for LintExplanation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { name, info } = self;
        // `find_lint` only constructs documented lints.
        let explanation = info.explanation().expect("documented lint");
        writeln!(f, "{name} ({}): {}", rendered_code(info), info.message())?;
        writeln!(f)?;
        // The explanation file is printed verbatim; it ends with a newline.
        write!(f, "{explanation}")?;
        writeln!(f)?;
        writeln!(f, "Suppress a specific case with `#[allow(lint({name}))]`.")
    }
}

/// The catalog listing printed by `<command> --list`. `command` is the tool name referenced in the
/// header (e.g. `move lint` or `sui move lint`), so it matches how the tool was actually invoked.
pub struct LintIndex<'a> {
    pub command: &'a str,
}

impl fmt::Display for LintIndex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Move lint index. Run `{} --explain <name>` for details on any lint.\n",
            self.command
        )?;
        let width = all_lints().map(|(name, _)| name.len()).max().unwrap_or(0);
        // Categories in `LinterDiagnosticCategory` order; empty ones are skipped.
        for category in LinterDiagnosticCategory::ALL {
            let mut any = false;
            for (name, info) in all_lints().filter(|(_, info)| info.category() == *category as u8) {
                if !any {
                    writeln!(f, "{}", category.name())?;
                    any = true;
                }
                writeln!(f, "    {name:width$}  {}", info.message())?;
            }
            if any {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Route `--explain` snapshots into their own folder so they don't mix with the `.move`/`.snap`
    // linter fixtures under `tests/`.
    fn explain_settings() -> insta::Settings {
        let mut settings = insta::Settings::clone_current();
        settings.set_snapshot_path("snapshots/explain");
        settings.set_prepend_module_to_snapshot(false);
        settings
    }

    #[test]
    fn every_lint_has_an_explanation() {
        let missing: Vec<_> = all_lints()
            .filter(|(_, info)| info.explanation().is_none())
            .map(|(name, info)| format!("{name} ({})", rendered_code(info)))
            .collect();
        assert!(
            missing.is_empty(),
            "lints without an explanation file under \
             src/diagnostics/explanations/<Category>_<CodeName>.md: {missing:?}"
        );
    }

    #[test]
    fn explain_output_per_lint() {
        explain_settings().bind(|| {
            for (name, _) in all_lints() {
                let explanation = find_lint(name).expect("documented lint");
                insta::assert_snapshot!(*name, explanation.to_string());
            }
        });
    }

    #[test]
    fn explain_index() {
        explain_settings().bind(|| {
            insta::assert_snapshot!(
                "index",
                LintIndex {
                    command: "move lint"
                }
                .to_string()
            );
        });
    }

    #[test]
    fn ids_and_names_are_unique() {
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for (name, info) in all_lints() {
            assert!(names.insert(*name), "duplicate lint name {name}");
            assert!(
                ids.insert((info.category(), info.code())),
                "duplicate lint id for {name}"
            );
        }
    }

    #[test]
    fn explain_hint_injected_when_command_set() {
        use crate::diagnostics::codes::{Severity, custom};
        use crate::diagnostics::{
            Diagnostic, Diagnostics, report_diagnostics_to_buffer, set_explain_command,
        };
        use crate::linters::LINT_WARNING_PREFIX;
        use crate::shared::files::MappedFiles;
        use move_ir_types::location::Loc;

        // Runs in its own process under nextest, so the process-global command name is isolated.
        set_explain_command("test move lint");
        let info = custom(
            LINT_WARNING_PREFIX,
            Severity::Warning,
            99,
            0,
            "possible owned object share",
        );
        let diag = Diagnostic::new(
            info,
            (Loc::invalid(), "here"),
            Vec::<(Loc, String)>::new(),
            Vec::<String>::new(),
        );
        let mut diags = Diagnostics::new();
        diags.add(diag);
        let rendered = String::from_utf8(report_diagnostics_to_buffer(
            &MappedFiles::empty(),
            diags,
            false,
        ))
        .unwrap();
        assert!(
            rendered.contains("test move lint --explain share_owned"),
            "expected `--explain` hint in output:\n{rendered}"
        );
    }

    #[test]
    fn find_by_name_and_code() {
        let explanation = find_lint("share_owned").expect("by name");
        assert_eq!(explanation.name, "share_owned");
        assert_eq!(find_lint("W99000").unwrap().name, "share_owned");
        assert_eq!(find_lint("Lint W99000").unwrap().name, "share_owned");
        assert_eq!(find_lint("99000").unwrap().name, "share_owned");
        // Escalated rendering (`#[deny(lint(...))]`, `--warnings-are-errors`) prints an `E` code.
        assert_eq!(find_lint("E99000").unwrap().name, "share_owned");
        assert_eq!(find_lint("Lint E99000").unwrap().name, "share_owned");
        assert!(find_lint("nonsense").is_none());
    }
}
