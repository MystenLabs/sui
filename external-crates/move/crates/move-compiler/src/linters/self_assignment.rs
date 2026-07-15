// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects and reports explicit self-assignments in code, such as `x = x;`, which are generally unnecessary
//! and could indicate potential errors or misunderstandings in the code logic.
use super::StyleCodes;
use crate::{
    diag,
    naming::ast::Var,
    typing::{
        ast::{self as T},
        visitor::{same_local, simple_visitor},
    },
};
use move_ir_types::location::Loc;
use move_proc_macros::growing_stack;

simple_visitor!(
    SelfAssignment,
    fn visit_exp_custom(&mut self, e: &T::Exp) -> bool {
        use T::UnannotatedExp_ as E;
        match &e.exp.value {
            E::Mutate(lhs, rhs) => check_mutate(self, e.exp.loc, lhs, rhs),
            E::Assign(lvalues, _, rhs) => check_assign(self, lvalues, rhs),
            _ => (),
        }
        false
    }
);

fn check_mutate(context: &mut Context, loc: Loc, lhs: &T::Exp, rhs: &T::Exp) {
    #[growing_stack]
    fn same_memory_location(lhs: &T::Exp, rhs: &T::Exp) -> Option<(Loc, Loc)> {
        use T::UnannotatedExp_ as E;
        let lhs = inner_exp(lhs);
        let rhs = inner_exp(rhs);
        match &lhs.exp.value {
            E::Unit { .. }
            | E::Value(_)
            | E::Constant(_, _)
            | E::ModuleCall(_)
            | E::Vector(_, _, _, _)
            | E::IfElse(_, _, _)
            | E::Match(_, _)
            | E::VariantMatch(_, _, _)
            | E::While(_, _, _)
            | E::Loop { .. }
            | E::Assign(_, _, _)
            | E::Mutate(_, _)
            | E::Return(_)
            | E::Abort(_)
            | E::Continue(_)
            | E::Give(_, _)
            | E::Dereference(_)
            | E::UnaryExp(_, _)
            | E::BinopExp(_, _, _, _)
            | E::Pack(_, _, _, _)
            | E::PackVariant(_, _, _, _, _)
            | E::ExpList(_)
            | E::TempBorrow(_, _)
            | E::Cast(_, _)
            | E::ErrorConstant { .. }
            | E::UnresolvedError => None,
            E::Block(s) | E::NamedBlock(_, s) => {
                debug_assert!(s.1.len() > 1);
                None
            }

            E::Move { var: l, .. } | E::Copy { var: l, .. } | E::Use(l) | E::BorrowLocal(_, l) => {
                same_local(l, rhs)
            }
            E::Builtin(b1, l) => {
                if !gives_memory_location(b1) {
                    return None;
                }
                match &rhs.exp.value {
                    E::Builtin(b2, r) if b1 == b2 => same_memory_location(l, r),
                    _ => None,
                }
            }
            E::Borrow(_, l, lfield) => match &rhs.exp.value {
                E::Borrow(_, r, rfield) if lfield == rfield => {
                    same_memory_location(l, r)?;
                    Some((lhs.exp.loc, rhs.exp.loc))
                }
                _ => None,
            },

            E::Annotate(_, _) => unreachable!(),
        }
    }

    let rhs = inner_exp(rhs);
    let rhs = match &rhs.exp.value {
        T::UnannotatedExp_::Dereference(inner) => inner,
        _ => rhs,
    };
    let Some((lhs_loc, rhs_loc)) = same_memory_location(lhs, rhs) else {
        return;
    };
    report_self_assignment(context, "mutation", loc, lhs_loc, rhs_loc);
}

fn check_assign(context: &mut Context, sp!(_, lvalues_): &T::LValueList, rhs: &T::Exp) {
    let vars = lvalues_.iter().map(lvalue_var).collect::<Vec<_>>();
    let rhs_items = exp_list_items(rhs);
    for (lhs_opt, rhs) in vars.into_iter().zip(rhs_items) {
        let Some((loc, lhs)) = lhs_opt else {
            continue;
        };
        if let Some((lhs_loc, rhs_loc)) = same_local(lhs, rhs) {
            report_self_assignment(context, "assignment", loc, lhs_loc, rhs_loc);
        }
    }
}

fn gives_memory_location(sp!(_, b_): &T::BuiltinFunction) -> bool {
    match b_ {
        T::BuiltinFunction_::Freeze(_) => true,
        T::BuiltinFunction_::Assert(_) => false,
    }
}

fn inner_exp(mut e: &T::Exp) -> &T::Exp {
    use T::UnannotatedExp_ as E;
    loop {
        match &e.exp.value {
            E::Annotate(inner, _) => e = inner,
            E::Block((_, seq)) | E::NamedBlock(_, (_, seq)) if seq.len() == 1 => {
                match &seq[0].value {
                    T::SequenceItem_::Seq(inner) => e = inner,
                    T::SequenceItem_::Declare(_) | T::SequenceItem_::Bind(_, _, _) => break e,
                }
            }
            _ => break e,
        }
    }
}

fn lvalue_var(sp!(loc, lvalue_): &T::LValue) -> Option<(Loc, &Var)> {
    use T::LValue_ as L;
    match &lvalue_ {
        L::Var { var, .. } => Some((*loc, var)),
        L::Ignore
        | L::Unpack(_, _, _, _)
        | L::BorrowUnpack(_, _, _, _, _)
        | L::UnpackVariant(_, _, _, _, _)
        | L::BorrowUnpackVariant(_, _, _, _, _, _) => None,
    }
}

fn exp_list_items(e: &T::Exp) -> Vec<&T::Exp> {
    match &inner_exp(e).exp.value {
        T::UnannotatedExp_::ExpList(items) => items
            .iter()
            .flat_map(|item| match item {
                T::ExpListItem::Single(e, _) => vec![e],
                T::ExpListItem::Splat(_, e, _) => exp_list_items(e),
            })
            .collect::<Vec<_>>(),
        _ => vec![e],
    }
}

fn report_self_assignment(context: &mut Context, case: &str, eloc: Loc, lloc: Loc, rloc: Loc) {
    let msg =
        format!("Unnecessary self-{case}. The {case} is redundant and will not change the value");
    context.add_diag(diag!(
        StyleCodes::SelfAssignment.diag_info(),
        (eloc, msg),
        (lloc, "This location"),
        (rloc, "Is the same as this location"),
    ));
}
