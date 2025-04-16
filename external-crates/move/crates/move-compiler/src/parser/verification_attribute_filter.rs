// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use crate::{
    diag,
    diagnostics::DiagnosticReporter,
    parser::{
        ast as P,
        filter::{filter_program, FilterContext},
    },
    shared::{known_attributes, CompilationEnv},
};

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

    // An AST element should be removed if:
    // * It is annotated #[verify_only] and verify mode is not set
    fn should_remove_by_attributes(&mut self, attrs: &[P::Attributes]) -> bool {
        use known_attributes::VerificationAttribute;
        let flattened_attrs: Vec<_> = attrs.iter().flat_map(verification_attributes).collect();
        let is_verify_only_loc = flattened_attrs.iter().map(|attr| attr.0).next();
        let should_remove = is_verify_only_loc.is_some();
        // TODO this is a bit of a hack
        // but we don't have a better way of suppressing this unless the filtering was done after
        // expansion
        // Ideally we would just have a warning filter scope here
        // (but again, need expansion for that)
        let silence_warning =
            !self.is_source_def || self.env.package_config(self.current_package).is_dependency;
        if !silence_warning {
            if let Some(loc) = is_verify_only_loc {
                let msg = format!(
                    "The '{}' attribute has been deprecated along with specification blocks",
                    VerificationAttribute::VERIFY_ONLY
                );
                self.reporter
                    .add_diag(diag!(Uncategorized::DeprecatedWillBeRemoved, (loc, msg)));
            }
        }
        should_remove
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

fn verification_attributes(attrs: &P::Attributes) -> Vec<(Loc, known_attributes::AttributeKind_)> {
    attrs
        .value
        .iter()
        .filter_map(|attr| match attr.value {
            P::Attribute_::VerifyOnly => {
                Some((attr.loc, known_attributes::AttributeKind_::VerifyOnly))
            }
            P::Attribute_::BytecodeInstruction
            | P::Attribute_::DefinesPrimitive(..)
            | P::Attribute_::Deprecation { .. }
            | P::Attribute_::Error { .. }
            | P::Attribute_::External { .. }
            | P::Attribute_::Syntax { .. }
            | P::Attribute_::Allow { .. }
            | P::Attribute_::LintAllow { .. }
            | P::Attribute_::Test
            | P::Attribute_::TestOnly
            | P::Attribute_::ExpectedFailure { .. }
            | P::Attribute_::RandomTest => None,
        })
        .collect()
}
