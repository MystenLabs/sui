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
    has_spec_code: bool,
    current_package: Option<Symbol>,
}

impl<'env> Context<'env> {
    fn new(env: &'env CompilationEnv) -> Self {
        let reporter = env.diagnostic_reporter_at_top_level();
        Self {
            env,
            reporter,
            is_source_def: false,
            has_spec_code: false,
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
    // * It is annotated #[spec_only] and verify mode is not set
    fn should_remove_by_attributes(&mut self, attrs: &[P::Attributes]) -> bool {
        if self.env.flags().is_spec() {
            return false;
        }
        use known_attributes::SpecAttribute;
        let flattened_attrs: Vec<_> = attrs.iter().flat_map(verification_attributes).collect();
        //
        let is_spec_only = flattened_attrs.iter().find(|(_, attr)| {
            matches!(attr, SpecAttribute::SpecOnly) || matches!(attr, SpecAttribute::Spec)
        });
        self.has_spec_code = self.has_spec_code || is_spec_only.is_some();
        is_spec_only.is_some()
    }

    fn should_remove_sequence_item(&self, item: &P::SequenceItem) -> bool {
        self.has_spec_code
            && match &item.value {
                P::SequenceItem_::Seq(exp) => should_remove_exp(exp),
                P::SequenceItem_::Declare(_, _) => false,
                P::SequenceItem_::Bind(bind_list, _, exp) => {
                    let is_call_to_spec = should_remove_exp(exp);
                    let is_spec_variable = bind_list.value.iter().any(|bind| match bind.value {
                        P::Bind_::Var(_, var) => {
                            // var ends in "_spec"
                            let name = var.0.value.as_str();
                            name.ends_with("_spec")
                        }
                        P::Bind_::Unpack(_, _) => false,
                    });

                    is_call_to_spec || is_spec_variable
                }
            }
    }
}

const REMOVED_FUNCTIONS: [&str; 9] = [
    "invariant",
    "old",
    "requires",
    "ensures",
    "asserts",
    "type_inv",
    "declare_global",
    "declare_global_mut",
    "global",
];
const REMOVED_METHODS: [&str; 2] = ["to_int", "to_real"];

fn should_remove_exp(exp: &Box<move_ir_types::location::Spanned<P::Exp_>>) -> bool {
    match &exp.value {
        P::Exp_::Call(name_access_chain, _) => {
            let name_access_chain_str = format!("{}", name_access_chain);
            let should_remove = REMOVED_FUNCTIONS
                .iter()
                .any(|&keyword| name_access_chain_str.ends_with(keyword));
            should_remove
        }
        P::Exp_::DotCall(_, _, name, _, _, _) => {
            let name_str = format!("{}", name);
            let should_remove = REMOVED_METHODS
                .iter()
                .any(|&keyword| name_str.ends_with(keyword));
            should_remove
        }
        P::Exp_::Assign(lhs, rhs) => should_remove_exp(lhs) || should_remove_exp(rhs),
        _ => false,
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

fn verification_attributes(attrs: &P::Attributes) -> Vec<(Loc, known_attributes::SpecAttribute)> {
    use known_attributes::KnownAttribute;
    attrs
        .value
        .iter()
        .filter_map(
            |attr| match KnownAttribute::resolve(attr.value.attribute_name().value)? {
                KnownAttribute::Spec(verify_attr) => Some((attr.loc, verify_attr)),
                _ => None,
            },
        )
        .collect()
}
