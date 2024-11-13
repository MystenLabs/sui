// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use crate::{
    parser::{
        ast as P,
        filter::{filter_program, FilterContext},
    },
    shared::{known_attributes, CompilationEnv},
};

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

    // An AST element should be removed if:
    // * It is annotated #[verify_only] and verify mode is not set
    fn should_remove_by_attributes(&mut self, attrs: &[P::Attributes]) -> bool {
        if self.env.flags().is_verifying() {
            return false;
        }
        use known_attributes::VerificationAttribute;
        let flattened_attrs: Vec<_> = attrs.iter().flat_map(verification_attributes).collect();
        let is_verify_only_loc = flattened_attrs
            .iter()
            .map(|attr| match attr {
                (loc, VerificationAttribute::VerifyOnly) => loc,
            })
            .next();
        is_verify_only_loc.is_some()
    }

    fn should_remove_sequence_item(&self, item: &P::SequenceItem) -> bool {
        match &item.value {
            P::SequenceItem_::Seq(exp) => should_remove_exp(exp),
            P::SequenceItem_::Declare(_, _) => false,
            P::SequenceItem_::Bind(_, _, exp) => should_remove_exp(exp),
        }
    }
}

fn should_remove_exp(exp: &Box<move_ir_types::location::Spanned<P::Exp_>>) -> bool {
    match &exp.value {
        P::Exp_::Call(name_access_chain, _) => {
            // get a string representation of name_access_chain using fmt display
            let name_access_chain_str = format!("{}", name_access_chain);
            // return true if name_access_chain_str ends with "verify"
            let should_remove = name_access_chain_str.ends_with("invariant");
            println!("name_access_chain_str: {}", name_access_chain_str);
            if should_remove {
                println!(
                    "Removing verification function call: {}",
                    name_access_chain_str
                );
            }
            should_remove
        }
        e => false,
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
