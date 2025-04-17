// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    cfgir::visitor::CFGIRVisitor,
    command_line::compiler::Visitor,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        warning_filters::WarningFilter,
    },
    typing::visitor::TypingVisitor,
};

pub mod abort_constant;
pub mod combinable_comparisons;
pub mod constant_naming;
pub mod equal_operands;
pub mod loop_without_exit;
pub mod meaningless_math_operation;
pub mod redundant_ref_deref;
pub mod self_assignment;
pub mod unnecessary_conditional;
pub mod unnecessary_unit;
pub mod unnecessary_while_loop;
pub mod unneeded_return;

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
        LinterDiagnosticCategory::Style,
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
        AbortWithoutConstant,
        LinterDiagnosticCategory::Style,
        "abort_without_constant",
        "'abort' or 'assert' without named constant"
    ),
    (
        LoopWithoutExit,
        LinterDiagnosticCategory::Suspicious,
        "loop_without_exit",
        "'loop' without 'break' or 'return'"
    ),
    (
        UnnecessaryConditional,
        LinterDiagnosticCategory::Complexity,
        "unnecessary_conditional",
        "'if' expression can be removed"
    ),
    (
        SelfAssignment,
        LinterDiagnosticCategory::Suspicious,
        "self_assignment",
        "assignment preserves the same value"
    ),
    (
        RedundantRefDeref,
        LinterDiagnosticCategory::Complexity,
        "redundant_ref_deref",
        "redundant reference/dereference"
    ),
    (
        UnnecessaryUnit,
        LinterDiagnosticCategory::Style,
        "unnecessary_unit",
        "unit `()` expression can be removed or simplified"
    ),
    (
        EqualOperands,
        LinterDiagnosticCategory::Suspicious,
        "always_equal_operands",
        "redundant, always-equal operands for binary operation"
    ),
    (
        CombinableComparisons,
        LinterDiagnosticCategory::Complexity,
        "combinable_comparisons",
        "comparison operations condition can be simplified"
    )
);

pub const ALLOW_ATTR_CATEGORY: &str = "lint";
pub const LINT_WARNING_PREFIX: &str = "Lint ";

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
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
    )
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    match level {
        LintLevel::None | LintLevel::Default => vec![],
        LintLevel::All => {
            vec![
                constant_naming::ConstantNaming.visitor(),
                unnecessary_while_loop::WhileTrueToLoop.visitor(),
                meaningless_math_operation::MeaninglessMathOperation.visitor(),
                unneeded_return::UnneededReturn.visitor(),
                abort_constant::AssertAbortNamedConstants.visitor(),
                loop_without_exit::LoopWithoutExit.visitor(),
                unnecessary_conditional::UnnecessaryConditional.visitor(),
                self_assignment::SelfAssignment.visitor(),
                redundant_ref_deref::RedundantRefDeref.visitor(),
                unnecessary_unit::UnnecessaryUnit.visitor(),
                equal_operands::EqualOperands.visitor(),
                combinable_comparisons::CombinableComparisons.visitor(),
            ]
        }
    }
}
