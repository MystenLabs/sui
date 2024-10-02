// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Lint to encourage the use of named constants with 'abort' and 'assert' for enhanced code readability.
//! Detects cases where non-constants are used and issues a warning.
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use crate::linters::StyleCodes;
use crate::{
    cfgir::{
        ast as G,
        visitor::{CFGIRVisitorConstructor, CFGIRVisitorContext},
    },
    diag,
    diagnostics::WarningFilters,
    editions::FeatureGate,
    hlir::ast as H,
    shared::CompilationEnv,
};

pub struct AssertAbortNamedConstants;

pub struct Context<'a> {
    package_name: Option<Symbol>,
    env: &'a mut CompilationEnv,
}

impl CFGIRVisitorConstructor for AssertAbortNamedConstants {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, program: &G::Program) -> Self::Context<'a> {
        let package_name = program
            .modules
            .iter()
            .next()
            .and_then(|(_, _, mdef)| mdef.package_name);
        Context { env, package_name }
    }
}

impl CFGIRVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_command_custom(&mut self, cmd: &H::Command) -> bool {
        if let H::Command_::Abort(loc, abort_exp) = &cmd.value {
            self.check_named_constant(abort_exp, *loc);
        }
        false
    }
}

impl Context<'_> {
    fn check_named_constant(&mut self, arg_exp: &H::Exp, loc: Loc) {
        let is_constant = matches!(
            &arg_exp.exp.value,
            H::UnannotatedExp_::Constant(_) | H::UnannotatedExp_::ErrorConstant { .. },
        );

        if !is_constant {
            let mut diag = diag!(
                StyleCodes::AbortWithoutConstant.diag_info(),
                (loc, "Prefer using a named constant.")
            );

            if self
                .env
                .supports_feature(self.package_name, FeatureGate::CleverAssertions)
            {
                diag.add_note("Consider using an error constant with the '#[error]' to allow for a more descriptive error.");
            }

            self.env.add_diag(diag);
        }
    }
}
