// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::{
    command_line::compiler::Visitor, diagnostics::codes::WarningFilter,
    linters::public_mut_tx_context::RequireMutableTxContext, typing::visitor::TypingVisitor,
};
pub mod public_mut_tx_context;
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
pub const REQUIRE_MUTABLE_TX_CONTEXT_FILTER_NAME: &str = "public_mut_tx_context";

pub const LINTER_DEFAULT_DIAG_CODE: u8 = 1;

pub enum LinterDiagCategory {
    RequireMutableTxContext,
}

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
        vec![WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::RequireMutableTxContext as u8,
            LINTER_DEFAULT_DIAG_CODE,
            Some(REQUIRE_MUTABLE_TX_CONTEXT_FILTER_NAME),
        )],
    )
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    match level {
        LintLevel::None => vec![],
        LintLevel::Default | LintLevel::All => {
            vec![public_mut_tx_context::RequireMutableTxContext::visitor(
                RequireMutableTxContext,
            )]
        }
    }
}
