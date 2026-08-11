// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    cfgir::visitor::{AbstractInterpreterVisitor, CFGIRVisitor},
    command_line::compiler::Visitor,
    diagnostics::{
        codes::{DiagnosticOrigin, DiagnosticsID},
        filter::FilterName,
    },
    shared::known_attributes::DiagnosticAttribute,
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
pub mod unused_return_value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintLevel {
    // No linters
    None,
    // Run only the default linters
    Default,
    // Run all linters
    All,
}

/// Categories shared by every lint origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LinterDiagnosticCategory {
    Correctness,
    Complexity,
    Suspicious,
    Deprecated,
    Style,
    Security,
    Conventions,
}

/// Declares a lint code enum and its `(category, code, filter_name)` table for one lint source.
/// Codes are positional and published; append only, never reorder or insert.
macro_rules! lints {
    (
        $enum_name:ident,
        $origin:expr,
        $filters_const:ident,
        $(
            ($lint_name:ident, $category:ident, $filter_name:expr, $code_msg:expr)
        ),* $(,)?
    ) => {
        #[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum $enum_name {
            DontStartAtZeroPlaceholder,
            $(
                $lint_name,
            )*
        }

        impl $enum_name {
            pub(crate) const ORIGIN: $crate::diagnostics::codes::DiagnosticOrigin = $origin;

            const fn category_code_and_message(&self) -> (u8, u8, &'static str) {
                let code = *self as u8;
                debug_assert!(code > 0);
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    $(Self::$lint_name => (
                        $crate::linters::LinterDiagnosticCategory::$category as u8,
                        code,
                        $code_msg,
                    ),)*
                }
            }

            const fn category_code_and_filter_name(&self) -> (u8, u8, &'static str) {
                let code = *self as u8;
                debug_assert!(code > 0);
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    $(Self::$lint_name => (
                        $crate::linters::LinterDiagnosticCategory::$category as u8,
                        code,
                        $filter_name,
                    ),)*
                }
            }

            pub(crate) const fn diag_info(
                &self,
            ) -> $crate::diagnostics::codes::DiagnosticInfo {
                let (category, code, msg) = self.category_code_and_message();
                $crate::diagnostics::codes::custom(
                    Self::ORIGIN,
                    $crate::diagnostics::codes::Severity::Warning,
                    category,
                    code,
                    msg,
                )
            }
        }

        const $filters_const: &[(u8, u8, &str)] = &[
            $(
                $enum_name::$lint_name.category_code_and_filter_name(),
            )*
        ];
    }
}
pub(crate) use lints;

lints!(
    StyleCodes,
    DiagnosticOrigin::Lint,
    CORE_LINT_WARNING_FILTERS,
    (
        ConstantNaming,
        Style,
        "constant_naming",
        "constant should follow naming convention"
    ),
    (
        WhileTrueToLoop,
        Style,
        "while_true",
        "unnecessary 'while (true)', replace with 'loop'"
    ),
    (
        MeaninglessMath,
        Complexity,
        "unnecessary_math",
        "math operator can be simplified"
    ),
    (UnneededReturn, Style, "unneeded_return", "unneeded return"),
    (
        AbortWithoutConstant,
        Style,
        "abort_without_constant",
        "'abort' or 'assert' without named constant"
    ),
    (
        LoopWithoutExit,
        Suspicious,
        "loop_without_exit",
        "'loop' without 'break' or 'return'"
    ),
    (
        UnnecessaryConditional,
        Complexity,
        "unnecessary_conditional",
        "'if' expression can be removed"
    ),
    (
        SelfAssignment,
        Suspicious,
        "self_assignment",
        "assignment preserves the same value"
    ),
    (
        RedundantRefDeref,
        Complexity,
        "redundant_ref_deref",
        "redundant reference/dereference"
    ),
    (
        UnnecessaryUnit,
        Style,
        "unnecessary_unit",
        "unit `()` expression can be removed or simplified"
    ),
    (
        EqualOperands,
        Suspicious,
        "always_equal_operands",
        "redundant, always-equal operands for binary operation"
    ),
    (
        CombinableComparisons,
        Complexity,
        "combinable_comparisons",
        "comparison operations condition can be simplified"
    ),
    (
        UnusedReturnValue,
        Suspicious,
        "unused_return_value",
        "return value of a non-mutating call is discarded"
    ),
);

pub(crate) fn filters_from_table(
    origin: DiagnosticOrigin,
    table: &[(u8, u8, &'static str)],
) -> Vec<(FilterName, Vec<DiagnosticsID>)> {
    let mut filters = vec![(
        Symbol::from(crate::diagnostics::filter::FILTER_ALL),
        vec![DiagnosticsID::all(origin)],
    )];
    filters.extend(table.iter().map(|(category, code, filter_name)| {
        (
            Symbol::from(*filter_name),
            vec![DiagnosticsID::exact(origin, *category, *code)],
        )
    }));
    filters
}

pub fn known_filters() -> (Option<Symbol>, Vec<(FilterName, Vec<DiagnosticsID>)>) {
    (
        Some(DiagnosticAttribute::LINT_SYMBOL),
        filters_from_table(StyleCodes::ORIGIN, CORE_LINT_WARNING_FILTERS),
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
                unused_return_value::UnusedReturnValue.visitor(),
            ]
        }
    }
}
