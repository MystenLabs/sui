// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Lookup and rendering for `move explain <CODE>`.

use crate::{
    diagnostics::codes::{
        COMPILER_DIAGNOSTICS, DiagnosticDescription, DiagnosticInfo, DiagnosticOrigin,
    },
    linters::{LinterDiagnosticCategory, MOVE_LINTS},
    sui_mode::{SUI_COMPILER_DIAGNOSTICS, linters::SUI_LINTS},
};
use std::{collections::BTreeMap, fmt, sync::LazyLock};

fn all_diagnostics() -> impl Iterator<Item = &'static DiagnosticDescription> {
    COMPILER_DIAGNOSTICS
        .iter()
        .chain(MOVE_LINTS.iter())
        .chain(SUI_COMPILER_DIAGNOSTICS.iter())
        .chain(SUI_LINTS.iter())
}

fn rendered_code(info: &DiagnosticInfo) -> String {
    info.clone().render().0
}

fn normalize_code(code: &str) -> String {
    let code = code.trim();
    let code = code
        .split_once('[')
        .and_then(|(prefix, rest)| {
            ["error", "warning", "note", "bug"]
                .iter()
                .any(|kind| prefix.eq_ignore_ascii_case(kind))
                .then(|| rest.strip_suffix(']'))
                .flatten()
        })
        .unwrap_or(code);
    let code = code
        .get(..5)
        .filter(|prefix| prefix.eq_ignore_ascii_case("lint "))
        .and_then(|_| code.get(5..))
        .unwrap_or(code);
    code.trim().to_ascii_uppercase()
}

static DIAGNOSTICS_BY_CODE: LazyLock<BTreeMap<String, &'static DiagnosticDescription>> =
    LazyLock::new(|| {
        all_diagnostics()
            .map(|diagnostic| (rendered_code(&diagnostic.info), diagnostic))
            .collect()
    });

#[derive(Debug, PartialEq, Eq)]
pub struct FindDiagnosticError(String);

impl fmt::Display for FindDiagnosticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown diagnostic code `{}`", self.0)
    }
}

pub struct DiagnosticExplanation {
    diagnostic: &'static DiagnosticDescription,
}

pub fn find_explanation(query: &str) -> Result<DiagnosticExplanation, FindDiagnosticError> {
    let code = normalize_code(query);
    if let Some(diagnostic) = DIAGNOSTICS_BY_CODE.get(&code).copied().or_else(|| {
        let elevated_warning = code.strip_prefix('E')?;
        DIAGNOSTICS_BY_CODE
            .get(&format!("W{elevated_warning}"))
            .copied()
    }) {
        Ok(DiagnosticExplanation { diagnostic })
    } else {
        Err(FindDiagnosticError(query.to_owned()))
    }
}

impl fmt::Display for DiagnosticExplanation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let DiagnosticDescription {
            info,
            explanation,
            lint_filter,
        } = self.diagnostic;
        writeln!(f, "{}: {}", rendered_code(info), info.message())?;
        writeln!(f)?;
        if let Some(explanation) = explanation {
            write!(f, "{explanation}")?;
            if !explanation.ends_with('\n') {
                writeln!(f)?;
            }
        } else {
            writeln!(
                f,
                "No detailed explanation is available for this diagnostic."
            )?;
        }
        if let Some(lint_filter) = lint_filter {
            writeln!(f)?;
            writeln!(
                f,
                "Suppress a specific case with `#[allow(lint({lint_filter}))]`."
            )?;
        }
        Ok(())
    }
}

pub struct DiagnosticIndex;

fn origin_name(origin: DiagnosticOrigin) -> &'static str {
    match origin {
        DiagnosticOrigin::Compiler => "Move compiler",
        DiagnosticOrigin::Lint => "Move lints",
        DiagnosticOrigin::SuiCompiler => "Sui compiler",
        DiagnosticOrigin::SuiLint => "Sui lints",
        DiagnosticOrigin::UpgradeCompatibility => "Upgrade compatibility",
    }
}

impl fmt::Display for DiagnosticIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Move diagnostic codes. Use a code with `explain` for details.\n"
        )?;

        for origin in [
            DiagnosticOrigin::Compiler,
            DiagnosticOrigin::Lint,
            DiagnosticOrigin::SuiCompiler,
            DiagnosticOrigin::SuiLint,
        ] {
            let entries: Vec<_> = all_diagnostics()
                .filter(|diagnostic| diagnostic.info.origin() == origin)
                .collect();
            if entries.is_empty() {
                continue;
            }

            writeln!(f, "{}", origin_name(origin))?;
            if matches!(origin, DiagnosticOrigin::Lint | DiagnosticOrigin::SuiLint) {
                for category in LinterDiagnosticCategory::ALL {
                    let category_entries = entries
                        .iter()
                        .copied()
                        .filter(|diagnostic| diagnostic.info.category() == *category as u8);
                    let mut category_entries = category_entries.peekable();
                    if category_entries.peek().is_none() {
                        continue;
                    }
                    writeln!(f, "  {}", category.name())?;
                    for diagnostic in category_entries {
                        writeln!(
                            f,
                            "    {:<9} {}",
                            rendered_code(&diagnostic.info),
                            diagnostic.info.message()
                        )?;
                    }
                }
            } else {
                for diagnostic in entries {
                    writeln!(
                        f,
                        "  {:<9} {}",
                        rendered_code(&diagnostic.info),
                        diagnostic.info.message()
                    )?;
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
