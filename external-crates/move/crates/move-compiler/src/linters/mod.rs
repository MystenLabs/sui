// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    cfgir::visitor::CFGIRVisitor,
    command_line::compiler::Visitor,
    diagnostics::codes::WarningFilter,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    typing::visitor::TypingVisitor,
};

pub mod constant_naming;
<<<<<<< HEAD
mod meaningless_math_operation;
mod unnecessary_while_loop;
mod unneeded_return;
pub mod abort_constant;
||||||| parent of 047a300271 (Rework a couple things + update tests)
pub mod meaningless_math_operation;
pub mod unnecessary_while_loop;
pub mod unnecessary_while_loop;
pub mod unnecessary_while_loop;
=======
pub mod meaningless_math_operation;
pub mod unnecessary_while_loop;
>>>>>>> 047a300271 (Rework a couple things + update tests)

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

macro_rules! lints {
    (
        $(
            ($enum_name:ident, $category:expr, $filter_name:expr, $code_msg:expr)
        ),* $(,)?
    ) => {
        #[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum StyleCodes {
            DontStartAtZeroPlaceholder,
            $(
                $enum_name,
            )*
        }

        impl StyleCodes {
            const fn category_code_and_message(&self) -> (u8, u8, &'static str) {
                let code = *self as u8;
                debug_assert!(code > 0);
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    $(Self::$enum_name => ($category as u8, code, $code_msg),)*
                }
            }

            const fn category_code_and_filter_name(&self) -> (u8, u8, &'static str) {
                let code = *self as u8;
                debug_assert!(code > 0);
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    $(Self::$enum_name => ($category as u8, code, $filter_name),)*
                }
            }

            const fn diag_info(&self) -> DiagnosticInfo {
                let (category, code, msg) = self.category_code_and_message();
                custom(
                    LINT_WARNING_PREFIX,
                    Severity::Warning,
                    category,
                    code,
                    msg,
                )
            }
        }

        const STYLE_WARNING_FILTERS: &[(u8, u8, &str)] = &[
            $(
                StyleCodes::$enum_name.category_code_and_filter_name(),
            )*
        ];
    }
}

lints!(
    (
        ConstantNaming,
        LinterDiagnosticCategory::Style,
        "constant_naming",
        "constant should follow naming convention"
    ),
    (
        WhileTrueToLoop,
        LinterDiagnosticCategory::Complexity,
        "while_true",
        "unnecessary 'while (true)', replace with 'loop'"
    ),
    (
        MeaninglessMath,
        LinterDiagnosticCategory::Complexity,
        "unnecessary_math",
        "math operator can be simplified"
    ),
    (
        UnneededReturn,
        LinterDiagnosticCategory::Style,
        "unneeded_return",
        "unneeded return"
    ),
    (
        AssertAbortNamedConstants,
        LinterDiagnosticCategory::Style,
        "abort_without_constant",
        "abort with named constant"
    ),
);

pub const ALLOW_ATTR_CATEGORY: &str = "lint";
pub const LINT_WARNING_PREFIX: &str = "Lint ";
<<<<<<< HEAD
||||||| parent of 3c440e0213 ([move][move-linter] implement abort constant rules)
pub const CONSTANT_NAMING_FILTER_NAME: &str = "constant_naming";
pub const CONSTANT_NAMING_DIAG_CODE: u8 = 1;
pub const WHILE_TRUE_TO_LOOP_FILTER_NAME: &str = "while_true";
pub const WHILE_TRUE_TO_LOOP_DIAG_CODE: u8 = 4;
pub const MEANINGLESS_MATH_FILTER_NAME: &str = "unnecessary_math";
pub const MEANINGLESS_MATH_DIAG_CODE: u8 = 8;
=======
pub const CONSTANT_NAMING_FILTER_NAME: &str = "constant_naming";
pub const CONSTANT_NAMING_DIAG_CODE: u8 = 1;
pub const WHILE_TRUE_TO_LOOP_FILTER_NAME: &str = "while_true";
pub const WHILE_TRUE_TO_LOOP_DIAG_CODE: u8 = 4;
pub const MEANINGLESS_MATH_FILTER_NAME: &str = "unnecessary_math";
pub const MEANINGLESS_MATH_DIAG_CODE: u8 = 8;
pub const ABORT_CONSTANT_FILTER_NAME: &str = "abort_constant";
pub const ABORT_CONSTANT_DIAG_CODE: u8 = 9;
>>>>>>> 3c440e0213 ([move][move-linter] implement abort constant rules)

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
<<<<<<< HEAD
        STYLE_WARNING_FILTERS
            .iter()
            .map(|(category, code, filter_name)| {
                WarningFilter::code(
                    Some(LINT_WARNING_PREFIX),
                    *category,
                    *code,
                    Some(filter_name),
                )
            })
            .collect(),
||||||| parent of 3c440e0213 ([move][move-linter] implement abort constant rules)
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
                WHILE_TRUE_TO_LOOP_DIAG_CODE,
                Some(WHILE_TRUE_TO_LOOP_FILTER_NAME),
            ),
            WarningFilter::code(
                Some(LINT_WARNING_PREFIX),
                LinterDiagnosticCategory::Complexity as u8,
                MEANINGLESS_MATH_DIAG_CODE,
                Some(MEANINGLESS_MATH_FILTER_NAME),
            ),
        ],
=======
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
                WHILE_TRUE_TO_LOOP_DIAG_CODE,
                Some(WHILE_TRUE_TO_LOOP_FILTER_NAME),
            ),
            WarningFilter::code(
                Some(LINT_WARNING_PREFIX),
                LinterDiagnosticCategory::Complexity as u8,
                MEANINGLESS_MATH_DIAG_CODE,
                Some(MEANINGLESS_MATH_FILTER_NAME),
            ),
            WarningFilter::code(
                Some(LINT_WARNING_PREFIX),
                LinterDiagnosticCategory::Style as u8,
                ABORT_CONSTANT_DIAG_CODE,
                Some(ABORT_CONSTANT_FILTER_NAME),
            ),
        ],
>>>>>>> 3c440e0213 ([move][move-linter] implement abort constant rules)
    )
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    match level {
        LintLevel::None | LintLevel::Default => vec![],
        LintLevel::All => {
            vec![
                constant_naming::ConstantNamingVisitor.visitor(),
                unnecessary_while_loop::WhileTrueToLoop.visitor(),
                meaningless_math_operation::MeaninglessMathOperation.visitor(),
<<<<<<< HEAD
                unneeded_return::UnneededReturnVisitor.visitor(),
||||||| parent of 3c440e0213 ([move][move-linter] implement abort constant rules)
=======
                abort_constant::AssertAbortNamedConstants.visitor(),
>>>>>>> 3c440e0213 ([move][move-linter] implement abort constant rules)
            ]
        }
    }
}
