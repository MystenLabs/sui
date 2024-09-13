// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects and suggests simplification for `if (c) e1 else e2` can be removed
use move_proc_macros::growing_stack;

use crate::expansion::ast::Value;
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

pub struct UnnecessaryConditional;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for UnnecessaryConditional {
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
        let UnannotatedExp_::IfElse(_, etrue, efalse) = &exp.exp.value else {
            return false;
        };
        let Some(vtrue) = extract_value(etrue) else {
            return false;
        };
        let Some(vfalse) = extract_value(efalse) else {
            return false;
        };

        match (&vtrue.value, &vfalse.value) {
            (Value_::Bool(v1 @ true), Value_::Bool(false))
            | (Value_::Bool(v1 @ false), Value_::Bool(true)) => {
                let negation = if *v1 { "" } else { "!" };
                let msg = format!(
                    "Detected an unnecessary conditional expression 'if (cond)'. Consider using \
                    the condition directly, i.e. '{negation}cond'",
                );
                self.env.add_diag(diag!(
                    StyleCodes::UnnecessaryConditional.diag_info(),
                    (exp.exp.loc, msg)
                ));
            }
            (v1, v2) if v1 == v2 => {
                let msg =
                    "Detected a redundant conditional expression 'if (..) v else v', where each \
                    branch results in the same value 'v'. Consider using the value directly";
                self.env.add_diag(diag!(
                    StyleCodes::UnnecessaryConditional.diag_info(),
                    (exp.exp.loc, msg),
                    (vtrue.loc, "This value"),
                    (vfalse.loc, "is the same as this value"),
                ));
            }
            _ => (),
        }

        //     if let (Some(if_bool), Some(else_bool)) = (
        //         extract_bool_literal_from_block(if_block),
        //         extract_bool_literal_from_block(else_block),
        //     ) {
        //         if if_bool != else_bool {
        //             let msg = format!(
        //                 "Detected a redundant conditional expression `if (...) {} else {}`. Consider using the condition directly.",
        //                 if_bool, else_bool
        //             );
        //             let diag = diag!(
        //                 StyleCodes::UnnecessaryConditional.diag_info(),
        //                 (exp.exp.loc, msg)
        //             );

        //             self.env.add_diag(diag);
        //         }
        //     }
        // }
        false
    }
}

#[growing_stack]
fn extract_value(block: &T::Exp) -> Option<&Value> {
    match &block.exp.value {
        UnannotatedExp_::Block((_, seq)) if seq.len() == 1 => extract_value_seq_item(&seq[0]),
        UnannotatedExp_::Value(v) => Some(v),
        UnannotatedExp_::Annotate(e, _) => extract_value(e),
        _ => None,
    }
}

#[growing_stack]
fn extract_value_seq_item(sp!(_, item_): &T::SequenceItem) -> Option<&Value> {
    match &item_ {
        SequenceItem_::Declare(_) | SequenceItem_::Bind(_, _, _) => None,
        SequenceItem_::Seq(e) => extract_value(e),
    }
}
