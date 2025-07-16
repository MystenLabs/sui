// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    parser::{
        ast::{self as P},
        filter::{filter_program, FilterContext},
    },
    shared::{known_attributes::VerificationAttribute, CompilationEnv},
};

use move_symbol_pool::Symbol;
use std::collections::BTreeSet;

struct Context<'env> {
    env: &'env CompilationEnv,
    is_source_def: bool,
    current_package: Option<Symbol>,
}

impl<'env> Context<'env> {
    fn new(env: &'env CompilationEnv) -> Self {
        Self {
            env,
            is_source_def: false,
            current_package: None,
        }
    }
}

impl FilterContext for Context<'_> {
    fn set_current_package(&mut self, package: Option<Symbol>) {
        self.current_package = package;
    }

    fn set_is_source_def(&mut self, is_source_def: bool) {
        self.is_source_def = is_source_def;
    }

    // An AST element should be removed if no compiler mode is set for it.
    fn should_remove_by_attributes(&mut self, attrs: &[P::Attributes]) -> bool {
        let modes = attrs
            .iter()
            .flat_map(|attr| attr.value.modes())
            .collect::<BTreeSet<_>>();

        if modes.is_empty() {
            return false;
        };

        let modes = modes
            .into_iter()
            .map(|mode| mode.value)
            .collect::<BTreeSet<_>>();

        let mut allowed_modes = self.env.modes().clone();

        // ADDON FOR SPECS. EXPERIMENTAL
        if self.env.verify_mode() { // modes contains VERIFY_ONLY
            allowed_modes.insert(VerificationAttribute::SPEC.into());
            allowed_modes.insert(VerificationAttribute::SPEC_ONLY.into());
        }

        // If the compiler mode intersects with these modes, we should keep this
        allowed_modes.intersection(&modes).next().is_none()
    }
}

//***************************************************************************
// Filtering of verification-annotated module members
//***************************************************************************

// This filters out all AST elements annotated with verify-only annotated from `prog`
// if the `verify` flag in `compilation_env` is not set. If the `verify` flag is set,
// no filtering is performed.
pub fn program(compilation_env: &CompilationEnv, prog: P::Program) -> P::Program {
    let mut context = Context::new(compilation_env);
    filter_program(&mut context, prog)
}
