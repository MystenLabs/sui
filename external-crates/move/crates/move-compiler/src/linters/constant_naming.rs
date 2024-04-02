// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! `ConstantNamingVisitor` enforces a naming convention for constants in Move programs,
//! requiring them to follow an ALL_CAPS_SNAKE_CASE format. This lint checks each constant's name
//! within a module against this convention.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast as T,
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use super::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX};

/// Diagnostic information for constant naming violations.
const CONSTANT_NAMING_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::ConstantNaming as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "Constant name should be in all caps, snake case, pascal case or upper camel case.",
);

pub struct ConstantNamingVisitor;
pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}
impl TypingVisitorConstructor for ConstantNamingVisitor {
    type Context<'a> = Context<'a>;

    fn context<'a>(
        env: &'a mut CompilationEnv,
        _program_info: &'a TypingProgramInfo,
        _program: &T::Program_,
    ) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit(&mut self, program: &mut T::Program_) {
        for module_def in program
            .modules
            .iter()
            .filter(|(_, _, mdef)| !mdef.attributes.is_test_or_test_only())
        {
            self.env
                .add_warning_filter_scope(module_def.2.warning_filter.clone());
            module_def
                .2
                .constants
                .iter()
                .for_each(|(loc, name, _constant)| {
                    if !is_valid_name(name.as_str()) {
                        let uid_msg = format!("'{}' should be named using UPPER_CASE_WITH_UNDERSCORES or PascalCase/UpperCamelCase",name.as_str());
                        let diagnostic = diag!(CONSTANT_NAMING_DIAG, (loc, uid_msg));
                        self.env.add_diag(diagnostic);
                    }
                });
        }
    }

    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }
}

/// Returns `true` if the string is in all caps snake case, including numeric characters.
fn is_valid_name(name: &str) -> bool {
    if name
        .chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_numeric())
    {
        return true;
    }
    // Check for PascalCase/UpperCamelCase
    // The string must start with an uppercase letter, and only contain alphanumeric characters,
    // with every new word starting with an uppercase letter.
    let mut chars = name.chars();
    chars.next().unwrap().is_uppercase() && chars.all(|c| c.is_alphanumeric())
}
