// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    cfgir::visitor::{AbstractInterpreterVisitor, CFGIRVisitor},
    command_line::compiler::Visitor,
    diagnostics::{
        codes::{DiagnosticInfo, DiagnosticsID, Severity, custom},
        filter::FilterName,
    },
    typing::visitor::TypingVisitor,
};

pub mod abort_constant;
pub mod combinable_comparisons;
pub mod constant_naming;
pub mod docs;
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

impl LinterDiagnosticCategory {
    /// All categories, in the order they should be displayed.
    pub const ALL: &'static [Self] = &[
        Self::Correctness,
        Self::Complexity,
        Self::Suspicious,
        Self::Deprecated,
        Self::Style,
        Self::Sui,
    ];

    pub fn try_from_u8(value: u8) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| *c as u8 == value)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Correctness => "Correctness",
            Self::Complexity => "Complexity",
            Self::Suspicious => "Suspicious",
            Self::Deprecated => "Deprecated",
            Self::Style => "Style",
            Self::Sui => "Sui",
        }
    }
}

macro_rules! lints {
    (
        $(
            ($enum_name:ident, $category:ident, $filter_name:expr, $code_msg:expr)
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
            pub(crate) const fn diag_info(&self) -> DiagnosticInfo {
                let code = *self as u8;
                debug_assert!(code > 0);
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    // A lint has an explanation iff
                    // `diagnostics/explanations/<Category>_<CodeName>.md` exists -- keyed by
                    // identifiers rather than the rendered code since the numeric portions are
                    // positional (see `codes!` in diagnostics/codes.rs).
                    $(Self::$enum_name => custom(
                        LINT_WARNING_PREFIX,
                        Severity::Warning,
                        LinterDiagnosticCategory::$category as u8,
                        code,
                        $code_msg,
                    ).with_explanation(move_proc_macros::optional_include_str!(
                        "src/diagnostics/explanations/", $category, "_", $enum_name, ".md"
                    )),)*
                }
            }
        }

        /// Every Move lint as `(filter name, diagnostic info)`, in code order.
        pub(crate) const MOVE_LINTS: &[(&str, DiagnosticInfo)] = &[
            $(($filter_name, StyleCodes::$enum_name.diag_info()),)*
        ];
    }
}

lints!(
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

pub fn known_filters() -> (Option<Symbol>, Vec<(FilterName, Vec<DiagnosticsID>)>) {
    let mut filters: Vec<(FilterName, Vec<DiagnosticsID>)> = vec![(
        Symbol::from(crate::diagnostics::filter::FILTER_ALL),
        vec![DiagnosticsID::all(Some(LINT_WARNING_PREFIX))],
    )];
    filters.extend(
        MOVE_LINTS
            .iter()
            .map(|(filter_name, info)| (Symbol::from(*filter_name), vec![info.id()])),
    );
    (Some(ALLOW_ATTR_CATEGORY.into()), filters)
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
