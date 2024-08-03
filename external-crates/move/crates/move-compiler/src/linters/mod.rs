// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    command_line::compiler::Visitor,
    diagnostics::codes::{custom, DiagnosticInfo, Severity, WarningFilter},
    linters::{constant_naming::ConstantNamingVisitor, unneeded_return::UnneededReturnVisitor},
    typing::visitor::TypingVisitor,
};

pub mod constant_naming;
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
            ($enum_name:ident, $filter_name:expr, $code_msg:expr)
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
            const fn code_and_message(&self) -> (u8, &'static str) {
                let code = *self as u8;
                debug_assert!(code > 0);
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    $(Self::$enum_name => (code, $code_msg),)*
                }
            }

            const fn code_and_filter_name(&self) -> (u8, &'static str) {
                let code = *self as u8;
                debug_assert!(code > 0);
                match self {
                    Self::DontStartAtZeroPlaceholder =>
                        panic!("ICE do not use placeholder error code"),
                    $(Self::$enum_name => (code, $filter_name),)*
                }
            }

            const fn diag_info(&self) -> DiagnosticInfo {
                let (code, msg) = self.code_and_message();
                custom(
                    LINT_WARNING_PREFIX,
                    Severity::Warning,
                    LinterDiagnosticCategory::Style as u8,
                    code,
                    msg,
                )
            }
        }

        const STYLE_WARNING_FILTERS: &[(u8, &str)] = &[
            $(
                StyleCodes::$enum_name.code_and_filter_name(),
            )*
        ];
    }
}

// Example usage:
lints!(
    (
        ConstantNaming,
        "constant_naming",
        "constant should follow naming convention"
    ),
    (UnneededReturn, "unneeded_return", "unneeded return"),
);

pub const ALLOW_ATTR_CATEGORY: &str = "lint";
pub const LINT_WARNING_PREFIX: &str = "Lint ";

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
        STYLE_WARNING_FILTERS
            .iter()
            .map(|(code, filter_name)| {
                WarningFilter::code(
                    Some(LINT_WARNING_PREFIX),
                    LinterDiagnosticCategory::Style as u8,
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
                constant_naming::ConstantNamingVisitor::visitor(ConstantNamingVisitor),
                unneeded_return::UnneededReturnVisitor::visitor(UnneededReturnVisitor),
            ]
        }
    }
}
