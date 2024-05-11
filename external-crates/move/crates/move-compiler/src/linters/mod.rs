// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    command_line::compiler::Visitor, diagnostics::codes::WarningFilter,
    linters::impossible_comparisons::ImpossibleDoubleComparison, typing::visitor::TypingVisitor,
};
pub mod impossible_comparisons;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintLevel {
    // No linters
    None,
    // Run only the default linters
    Default,
    // Run all linters
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LinterDiagnosticCategory {
    Correctness,
    Complexity,
    Suspicious,
    Deprecated,
    Style,
    Sui = 99,
}

pub const ALLOW_ATTR_CATEGORY: &str = "lint";
pub const LINT_WARNING_PREFIX: &str = "Lint ";
pub const DOUBLE_COMPARISON_FILTER_NAME: &str = "double_comparison";

pub const LINTER_DEFAULT_DIAG_CODE: u8 = 1;
pub const LINTER_DOUBLE_COMPARISON_DIAG_CODE: u8 = 11;

pub enum LinterDiagCategory {
    Correctness,
}

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
        vec![WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::Correctness as u8,
            LINTER_DOUBLE_COMPARISON_DIAG_CODE,
            Some(DOUBLE_COMPARISON_FILTER_NAME),
        )],
    )
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    match level {
        LintLevel::None => vec![],
        LintLevel::Default | LintLevel::All => {
            vec![impossible_comparisons::ImpossibleDoubleComparison::visitor(
                ImpossibleDoubleComparison,
            )]
        }
    }
}
