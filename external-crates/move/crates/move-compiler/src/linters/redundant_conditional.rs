// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects and suggests simplification for `if c { true } else { false }` and its reverse pattern.
//! Encourages using the condition directly (or its negation) for clearer and more concise code.
use crate::linters::StyleCodes;
use crate::{
    diag,
    diagnostics::WarningFilters,
    expansion::ast::Value_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, SequenceItem_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};

pub struct RedundantConditional;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for RedundantConditional {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }
    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        if let UnannotatedExp_::IfElse(_, if_block, else_block) = &exp.exp.value {
            if let (Some(if_bool), Some(else_bool)) = (
                extract_bool_literal_from_block(if_block),
                extract_bool_literal_from_block(else_block)
            ) {
                if if_bool != else_bool {
                    let msg = format!(
                        "Detected a redundant conditional expression `if (...) {} else {}`. Consider using the condition directly.",
                        if_bool, else_bool
                    );
                    let diag = diag!(
                                StyleCodes::RedundantConditional.diag_info(),
                                (exp.exp.loc, msg)
                            );

                    self.env.add_diag(diag);
                }
            }
        }
        false
    }
}

fn extract_bool_literal_from_block(block: &T::Exp) -> Option<bool> {
    match &block.exp.value {
        UnannotatedExp_::Block(seq) if seq.1.len() == 1 => {
            extract_bool_from_sequence_item(&seq.1[0].value)
        }
        UnannotatedExp_::Value(sp!(_, Value_::Bool(b))) => Some(*b),
        UnannotatedExp_::Annotate(anno_exp, _) => {
            if let sp!(_, UnannotatedExp_::Value(sp!(_, Value_::Bool(b)))) = anno_exp.exp {
                Some(b)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_bool_from_sequence_item(item: &SequenceItem_) -> Option<bool> {
    if let SequenceItem_::Seq(seq_exp) = item {
        if let sp!(_, UnannotatedExp_::Value(sp!(_, Value_::Bool(b)))) = &seq_exp.exp {
            return Some(*b);
        }
    }
    None
}
