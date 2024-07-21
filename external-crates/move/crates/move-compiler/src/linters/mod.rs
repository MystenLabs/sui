// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    command_line::compiler::Visitor, diagnostics::codes::WarningFilter,
    linters::constant_naming::ConstantNamingVisitor,
    linters::meaningless_math_operation::MeaninglessMathOperation, typing::visitor::TypingVisitor,
};
use move_symbol_pool::Symbol;

pub mod constant_naming;
pub mod meaningless_math_operation;

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
pub const CONSTANT_NAMING_FILTER_NAME: &str = "constant_naming";
pub const CONSTANT_NAMING_DIAG_CODE: u8 = 1;

pub const MEANINGLESS_MATH_OP_FILTER_NAME: &str = "meaningless_math_op";
pub const MEANINGLESS_MATH_OP_DIAG_CODE: u8 = 6;

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
        vec![
            WarningFilter::code(
                Some(LINT_WARNING_PREFIX),
                LinterDiagnosticCategory::Style as u8,
                CONSTANT_NAMING_DIAG_CODE,
                Some(CONSTANT_NAMING_FILTER_NAME),
            ),
            WarningFilter::code(
                Some(LINT_WARNING_PREFIX),
                LinterDiagnosticCategory::Complexity as u8,
                MEANINGLESS_MATH_OP_DIAG_CODE,
                Some(MEANINGLESS_MATH_OP_FILTER_NAME),
            ),
        ],
    )
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    match level {
        LintLevel::None => vec![],
        LintLevel::Default => vec![],
        LintLevel::All => {
            vec![
                constant_naming::ConstantNamingVisitor::visitor(ConstantNamingVisitor),
                meaningless_math_operation::MeaninglessMathOperation::visitor(
                    MeaninglessMathOperation,
                ),
            ]
        }
    }
}
