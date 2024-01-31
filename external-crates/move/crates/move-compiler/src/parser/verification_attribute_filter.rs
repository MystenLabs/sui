// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::Loc;

use crate::{
    diag,
    parser::{
        ast as P,
        filter::{filter_program, FilterContext},
    },
    shared::{known_attributes, CompilationEnv},
};

struct Context<'env> {
    env: &'env mut CompilationEnv,
}

impl<'env> Context<'env> {
    fn new(compilation_env: &'env mut CompilationEnv) -> Self {
        Self {
            env: compilation_env,
        }
    }
}

impl FilterContext for Context<'_> {
    fn should_remove_by_attributes(
        &mut self,
        attrs: &[P::Attributes],
        _is_source_def: bool,
    ) -> bool {
        should_remove_node(self.env, attrs)
    }
}

//***************************************************************************
// Filtering of verification-annotated module members
//***************************************************************************

// This filters out all AST elements annotated with verify-only annotated from `prog`
// if the `verify` flag in `compilation_env` is not set. If the `verify` flag is set,
// no filtering is performed.
pub fn program(compilation_env: &mut CompilationEnv, prog: P::Program) -> P::Program {
    let mut context = Context::new(compilation_env);
    filter_program(&mut context, prog)
}

// An AST element should be removed if:
// * It is annotated #[verify_only] and verify mode is not set
fn should_remove_node(env: &mut CompilationEnv, attrs: &[P::Attributes]) -> bool {
    use known_attributes::VerificationAttribute;
    let flattened_attrs: Vec<_> = attrs.iter().flat_map(verification_attributes).collect();
    let is_verify_only_loc = flattened_attrs
        .iter()
        .map(|attr| match attr {
            (loc, VerificationAttribute::VerifyOnly) => loc,
        })
        .next();
    if let Some(loc) = is_verify_only_loc {
        let msg = format!(
            "The '{}' attribute has been deprecated along with specification blocks",
            VerificationAttribute::VERIFY_ONLY
        );
        env.add_diag(diag!(Uncategorized::DeprecatedWillBeRemoved, (*loc, msg)));
        true
    } else {
        false
    }
}

fn verification_attributes(
    attrs: &P::Attributes,
) -> Vec<(Loc, known_attributes::VerificationAttribute)> {
    use known_attributes::KnownAttribute;
    attrs
        .value
        .iter()
        .filter_map(
            |attr| match KnownAttribute::resolve(attr.value.attribute_name().value)? {
                KnownAttribute::Verification(verify_attr) => Some((attr.loc, verify_attr)),
                _ => None,
            },
        )
        .collect()
}
