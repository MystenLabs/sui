// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    command_line::compiler::Visitor, diagnostics::codes::WarningFilter,
    linters::combinable_bool_conditions::CombinableBool, typing::visitor::TypingVisitor,
};
pub mod combinable_bool_conditions;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintLevel {
    // No linters
    None,
    // Run only the default linters
    Default,
    // Run all linters
    All,
}

pub const ALLOW_ATTR_CATEGORY: &str = "lint";
pub const LINT_WARNING_PREFIX: &str = "Lint ";
pub const COMBINABLE_BOOL_FILTER_NAME: &str = "combinable_bool_conditions";

pub const LINTER_DEFAULT_DIAG_CODE: u8 = 1;

pub enum LinterDiagCategory {
    CombinableBool,
}

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
        vec![WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::CombinableBool as u8,
            LINTER_DEFAULT_DIAG_CODE,
            Some(COMBINABLE_BOOL_FILTER_NAME),
        )],
    )
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    match level {
        LintLevel::None => vec![],
        LintLevel::Default | LintLevel::All => {
            vec![combinable_bool_conditions::CombinableBool::visitor(
                CombinableBool,
            )]
        }
    }
}
