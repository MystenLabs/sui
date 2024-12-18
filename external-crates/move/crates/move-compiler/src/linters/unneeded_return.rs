// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! `UnneededReturnVisitor` enforces that users don't write `return <exp>` where `<exp>` is a
//! value-like thing.

use crate::{
    diag,
    expansion::ast::ModuleIdent,
    linters::StyleCodes,
    parser::ast::FunctionName,
    typing::{ast as T, visitor::simple_visitor},
};

use move_ir_types::location::Loc;
use move_proc_macros::growing_stack;

use std::collections::VecDeque;

simple_visitor!(
    UnneededReturn,
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        if let T::FunctionBody_::Defined((_, seq)) = &fdef.body.value {
            tail_block(self, seq);
        };
        true
    }
);

/// Recur down the tail (last) position of the sequence, looking for returns that
/// might occur in the function's taul/return position..
#[growing_stack]
fn tail_block(context: &mut Context, seq: &VecDeque<T::SequenceItem>) {
    let Some(last) = seq.back() else { return };
    match &last.value {
        T::SequenceItem_::Seq(exp) => tail(context, exp),
        // These don't make sense and shouldn't occur, but are irrelevant to us.
        T::SequenceItem_::Declare(_) | T::SequenceItem_::Bind(_, _, _) => (),
    }
}

/// Recur down the tail (last) position of each expression, looking for returns that
/// might occur in the function's taul/return position.
#[growing_stack]
fn tail(context: &mut Context, exp: &T::Exp) {
    match &exp.exp.value {
        T::UnannotatedExp_::IfElse(_, conseq, alt_opt) => {
            tail(context, conseq);
            if let Some(alt) = alt_opt {
                tail(context, alt);
            }
        }
        T::UnannotatedExp_::Match(_, arms) => {
            for arm in &arms.value {
                tail(context, &arm.value.rhs);
            }
        }
        T::UnannotatedExp_::VariantMatch(_, _, arms) => {
            for (_, rhs) in arms {
                tail(context, rhs);
            }
        }
        T::UnannotatedExp_::NamedBlock(_, (_, seq)) => {
            tail_block(context, seq);
        }
        T::UnannotatedExp_::Block((_, seq)) => {
            tail_block(context, seq);
        }
        T::UnannotatedExp_::Return(rhs) => {
            if returnable_value(context, rhs) {
                report_unneeded_return(context, exp.exp.loc);
            }
        }

        // These cases we don't care about, because they are:
        // - loops
        // - effects
        // - values already
        T::UnannotatedExp_::Builtin(_, _)
        | T::UnannotatedExp_::Vector(_, _, _, _)
        | T::UnannotatedExp_::While(_, _, _)
        | T::UnannotatedExp_::Loop { .. }
        | T::UnannotatedExp_::Assign(_, _, _)
        | T::UnannotatedExp_::Mutate(_, _)
        | T::UnannotatedExp_::Abort(_)
        | T::UnannotatedExp_::Give(_, _)
        | T::UnannotatedExp_::Continue(_)
        | T::UnannotatedExp_::Unit { .. }
        | T::UnannotatedExp_::Value(_)
        | T::UnannotatedExp_::Move { .. }
        | T::UnannotatedExp_::Copy { .. }
        | T::UnannotatedExp_::Use(_)
        | T::UnannotatedExp_::Constant(_, _)
        | T::UnannotatedExp_::ModuleCall(_)
        | T::UnannotatedExp_::Dereference(_)
        | T::UnannotatedExp_::UnaryExp(_, _)
        | T::UnannotatedExp_::BinopExp(_, _, _, _)
        | T::UnannotatedExp_::Pack(_, _, _, _)
        | T::UnannotatedExp_::PackVariant(_, _, _, _, _)
        | T::UnannotatedExp_::ExpList(_)
        | T::UnannotatedExp_::Borrow(_, _, _)
        | T::UnannotatedExp_::TempBorrow(_, _)
        | T::UnannotatedExp_::BorrowLocal(_, _)
        | T::UnannotatedExp_::Cast(_, _)
        | T::UnannotatedExp_::Annotate(_, _)
        | T::UnannotatedExp_::ErrorConstant { .. }
        | T::UnannotatedExp_::UnresolvedError => (),
    }
}

/// Indicates if the expression is "value"-like, in that it produces a value. This is just to
/// reduce noise for the lint, because things like `return loop { abort 0 }` is technically an
/// unnecessary return, but we don't need to complain about weird code like that.
#[growing_stack]
fn returnable_value(context: &mut Context, exp: &T::Exp) -> bool {
    match &exp.exp.value {
        T::UnannotatedExp_::Return(rhs) => {
            if returnable_value(context, rhs) {
                report_unneeded_return(context, exp.exp.loc);
            };
            false
        }

        T::UnannotatedExp_::BinopExp(lhs, _, _, rhs) => {
            returnable_value(context, lhs) && returnable_value(context, rhs)
        }
        T::UnannotatedExp_::ExpList(values) => values.iter().all(|v| match v {
            T::ExpListItem::Single(exp, _) => returnable_value(context, exp),
            T::ExpListItem::Splat(_, _, _) => false,
        }),
        T::UnannotatedExp_::Borrow(_, exp, _)
        | T::UnannotatedExp_::Dereference(exp)
        | T::UnannotatedExp_::UnaryExp(_, exp)
        | T::UnannotatedExp_::TempBorrow(_, exp)
        | T::UnannotatedExp_::Cast(exp, _)
        | T::UnannotatedExp_::Annotate(exp, _) => returnable_value(context, exp),

        T::UnannotatedExp_::Pack(_, _, _, _)
        | T::UnannotatedExp_::PackVariant(_, _, _, _, _)
        | T::UnannotatedExp_::Unit { .. }
        | T::UnannotatedExp_::Value(_)
        | T::UnannotatedExp_::Move { .. }
        | T::UnannotatedExp_::Copy { .. }
        | T::UnannotatedExp_::Use(_)
        | T::UnannotatedExp_::Constant(_, _)
        | T::UnannotatedExp_::ModuleCall(_)
        | T::UnannotatedExp_::Builtin(_, _)
        | T::UnannotatedExp_::BorrowLocal(_, _)
        | T::UnannotatedExp_::ErrorConstant { .. }
        | T::UnannotatedExp_::Vector(_, _, _, _) => true,

        T::UnannotatedExp_::IfElse(_, _, _)
        | T::UnannotatedExp_::Match(_, _)
        | T::UnannotatedExp_::VariantMatch(_, _, _) => true,

        // While loops can't yield values, so there should already be other errors.
        T::UnannotatedExp_::While(_, _, _) => false,

        // Non-while loops _can_ yield values, and should never appear after a return if the
        // value is intended.
        T::UnannotatedExp_::Loop { .. } => true,

        T::UnannotatedExp_::NamedBlock(_, (_, seq)) | T::UnannotatedExp_::Block((_, seq)) => {
            let Some(last) = seq.back() else { return false };
            match &last.value {
                T::SequenceItem_::Seq(exp) => returnable_value(context, exp),
                T::SequenceItem_::Declare(_) | T::SequenceItem_::Bind(_, _, _) => false,
            }
        }

        // These don't really make sense in return position, so there should already be other
        // errors.
        T::UnannotatedExp_::Assign(_, _, _)
        | T::UnannotatedExp_::Mutate(_, _)
        | T::UnannotatedExp_::Abort(_)
        | T::UnannotatedExp_::Give(_, _)
        | T::UnannotatedExp_::Continue(_) => false,
        T::UnannotatedExp_::UnresolvedError => false,
    }
}

fn report_unneeded_return(context: &mut Context, loc: Loc) {
    context.add_diag(diag!(
        StyleCodes::UnneededReturn.diag_info(),
        (
            loc,
            "Remove unnecessary 'return', the expression is already in a 'return' position"
        )
    ));
}
