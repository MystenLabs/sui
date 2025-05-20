// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::DiagnosticReporter,
    parser::{
        ast as P,
        filter::{FilterContext, filter_program},
    },
    shared::{CompilationEnv, known_attributes::ModeAttribute},
};

use move_ir_types::location::{Loc, sp};
use move_symbol_pool::Symbol;

use std::collections::BTreeSet;

struct Context<'env> {
    env: &'env CompilationEnv,
    reporter: DiagnosticReporter<'env>,
    is_source_def: bool,
    current_package: Option<Symbol>,
}

impl<'env> Context<'env> {
    fn new(env: &'env CompilationEnv) -> Self {
        let reporter = env.diagnostic_reporter_at_top_level();
        Self {
            env,
            reporter,
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

        // TODO: This is a bit of a hack but we don't have a better way of suppressing this unless
        // the filtering was done after expansion. We could also us a filtering warning scope here.
        let silence_warning =
            !self.is_source_def || self.env.package_config(self.current_package).is_dependency;

        if !silence_warning {
            // Report `verify_only` deprecation -- but only for the first one.
            if let Some(sp!(loc, _)) =
                modes.get(&sp(Loc::invalid(), ModeAttribute::VERIFY_ONLY.into()))
            {
                let msg = format!(
                    "The '{}' attribute has been deprecated along with specification blocks",
                    ModeAttribute::VERIFY_ONLY
                );
                self.reporter
                    .add_diag(diag!(Uncategorized::DeprecatedWillBeRemoved, (*loc, msg)));
            }
        }

        let modes = modes
            .into_iter()
            .map(|mode| mode.value)
            .collect::<BTreeSet<_>>();

        // If the compiler mode intersects with these modes, we should keep this
        self.env.modes().intersection(&modes).next().is_none()
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
