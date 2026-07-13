// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    cfgir::visitor::{AbstractInterpreterVisitor, CFGIRVisitor},
    command_line::compiler::Visitor,
    diagnostics::{codes::DiagnosticsID, filter::FilterName},
    editions::Flavor,
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

/// Categories shared by every lint source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LinterDiagnosticCategory {
    Correctness,
    Complexity,
    Suspicious,
    Deprecated,
    Style,
}

pub(crate) const fn lint_category_code(category: LinterDiagnosticCategory) -> u8 {
    let category = category as u8;
    assert!(category <= 9);
    category
}

/// Each flavor's lints carry the flavor's letter in their rendered code, e.g. Sui's `Lint WS2001`.
pub(crate) const fn lint_code_tag(flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::Core => "",
        Flavor::Sui => "S",
    }
}

/// Declares a lint code enum and its `(category, code, filter_name)` table for one lint source.
/// Codes are positional and published — append-only, never reorder or insert.
macro_rules! lints {
    (
        $enum_name:ident,
        $code_tag:expr,
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
            const fn category_code_and_message(&self) -> (u8, u8, &'static str) {
                let code = *self as u8;
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    $(Self::$lint_name => (
                        $crate::linters::lint_category_code(
                            $crate::linters::LinterDiagnosticCategory::$category,
                        ),
                        code,
                        $code_msg,
                    ),)*
                }
            }

            const fn category_code_and_filter_name(&self) -> (u8, u8, &'static str) {
                let code = *self as u8;
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    $(Self::$lint_name => (
                        $crate::linters::lint_category_code(
                            $crate::linters::LinterDiagnosticCategory::$category,
                        ),
                        code,
                        $filter_name,
                    ),)*
                }
            }

            pub(crate) const fn diag_info(&self) -> $crate::diagnostics::codes::DiagnosticInfo {
                let (category, code, msg) = self.category_code_and_message();
                $crate::diagnostics::codes::custom(
                    $crate::linters::LINT_WARNING_PREFIX,
                    $crate::diagnostics::codes::Severity::Warning,
                    category,
                    code,
                    msg,
                )
                .with_code_tag($code_tag)
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
    CoreLintCode,
    lint_code_tag(Flavor::Core),
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

pub const ALLOW_ATTR_CATEGORY: &str = "lint";
pub const LINT_WARNING_PREFIX: &str = "Lint ";

pub(crate) fn filters_from_table(
    code_tag: &'static str,
    table: &[(u8, u8, &'static str)],
) -> Vec<(FilterName, Vec<DiagnosticsID>)> {
    // The `all` wildcard spans code tags, so both lint sources register the same id for it;
    // `add_custom_known_filters` dedups the second registration.
    let mut filters = vec![(
        Symbol::from(crate::diagnostics::filter::FILTER_ALL),
        vec![DiagnosticsID::all(Some(LINT_WARNING_PREFIX))],
    )];
    filters.extend(table.iter().map(|(category, code, filter_name)| {
        (
            Symbol::from(*filter_name),
            vec![
                DiagnosticsID::exact(Some(LINT_WARNING_PREFIX), *category, *code)
                    .with_code_tag(code_tag),
            ],
        )
    }));
    filters
}

pub fn known_filters() -> (Option<Symbol>, Vec<(FilterName, Vec<DiagnosticsID>)>) {
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
        filters_from_table(lint_code_tag(Flavor::Core), CORE_LINT_WARNING_FILTERS),
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

#[cfg(test)]
mod tests {
    use super::*;

    // A failure means a table edit renumbered existing lints — append instead.
    #[test]
    fn core_lint_code_assignments_are_stable() {
        assert_eq!(lint_code_tag(Flavor::Core), "");
        let expected: &[(u8, u8, &str)] = &[
            (4, 1, "constant_naming"),
            (4, 2, "while_true"),
            (1, 3, "unnecessary_math"),
            (4, 4, "unneeded_return"),
            (4, 5, "abort_without_constant"),
            (2, 6, "loop_without_exit"),
            (1, 7, "unnecessary_conditional"),
            (2, 8, "self_assignment"),
            (1, 9, "redundant_ref_deref"),
            (4, 10, "unnecessary_unit"),
            (2, 11, "always_equal_operands"),
            (1, 12, "combinable_comparisons"),
            (2, 13, "unused_return_value"),
        ];
        assert_eq!(CORE_LINT_WARNING_FILTERS, expected);
    }
}
