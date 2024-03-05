// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! `ConstantNamingVisitor` enforces a naming convention for constants in Move programs,
//! requiring them to follow an ALL_CAPS_SNAKE_CASE format. This lint checks each constant's name
//! within a module against this convention.
use crate::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    sui_mode::linters::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX},
    typing::{ast as T, visitor::TypingVisitor},
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

/// Diagnostic information for constant naming violations.
const CONSTANT_NAMING_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::ConstantNaming as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "Constant name should be in all caps and snake case.",
);

pub struct ConstantNamingVisitor;

impl TypingVisitor for ConstantNamingVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        _program_info: &TypingProgramInfo,
        program: &mut T::Program_,
    ) {
        for module_def in program
            .modules
            .iter()
            .filter(|(_, _, mdef)| !mdef.attributes.is_test_or_test_only())
        {
            env.add_warning_filter_scope(module_def.2.warning_filter.clone());
            module_def
                .2
                .constants
                .iter()
                .for_each(|(loc, name, _constant)| {
                    check_constant_naming(env, *name, loc);
                });
            env.pop_warning_filter_scope();
        }
    }
}

/// Checks if a constant's name adheres to the all caps snake case naming convention.
fn check_constant_naming(env: &mut CompilationEnv, name: Symbol, loc: Loc) {
    if !is_all_caps_snake_case(name.as_str()) {
        let uid_msg = format!(
            "{} should be snaked cased and all caps, e.g. UPPER_CASE_WITH_UNDERSCORES",
            name.as_str()
        );

        let diagnostic = diag!(CONSTANT_NAMING_DIAG, (loc, uid_msg));
        env.add_diag(diagnostic);
    }
}

/// Returns `true` if the string is in all caps snake case, including numeric characters.
fn is_all_caps_snake_case(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_numeric())
        && name.chars().any(char::is_alphabetic)
}
