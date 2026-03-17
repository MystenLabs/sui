// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pre-expansion analysis for macro bodies: detects `let mut` bindings where the variable is never
//! mutated within the macro definition. This catches macro-author errors at the definition site,
//! complementing the CFGIR-level blanket suppression of `unused_let_mut` for macro-expanded code.
//!
//! The analysis runs at the naming level (before type-checking), so it cannot detect mutation
//! through method calls with `&mut self` — it conservatively treats all method receivers as
//! potentially mutated.

use std::collections::BTreeMap;

use crate::{
    diag,
    expansion::ast::{self as E, Mutability},
    naming::ast::{self as N},
};
use move_ir_types::location::*;

/// Checks a macro function body for `let mut` bindings that are never mutated.
/// Called from the naming phase after unused binding analysis.
pub(crate) fn check(reporter: &crate::diagnostics::DiagnosticReporter, f: &N::Function) {
    let body = match &f.body.value {
        N::FunctionBody_::Defined(seq) => seq,
        N::FunctionBody_::Native => return,
    };
    // Collect all `let mut` variables that are internal (non-parameter, non-$, non-underscore).
    // Maps Var_ -> (var_loc, mut_keyword_loc)
    let mut mut_vars: BTreeMap<N::Var_, (Loc, Loc)> = BTreeMap::new();
    collect_seq(&mut mut_vars, body);
    if mut_vars.is_empty() {
        return;
    }
    // Walk the body to find mutations, removing variables from mut_vars when mutated.
    check_seq(&mut mut_vars, body);
    // Report remaining unmutated `let mut` variables.
    for (var_, (decl_loc, mut_loc)) in mut_vars {
        let name = var_.name;
        let decl_msg = format!("The variable '{name}' is never used mutably in this macro");
        let mut_msg = "Consider removing the 'mut' declaration here";
        reporter.add_diag(diag!(
            UnusedItem::MutModifier,
            (decl_loc, decl_msg),
            (mut_loc, mut_msg)
        ));
    }
}

// ================================================================================================
// Collection: find all `let mut` bindings in the macro body
// ================================================================================================

fn collect_seq(mut_vars: &mut BTreeMap<N::Var_, (Loc, Loc)>, seq: &N::Sequence) {
    for sp!(_, item_) in &seq.1 {
        match item_ {
            N::SequenceItem_::Bind(sp!(_, lvalues), e) => {
                collect_lvalues(mut_vars, lvalues);
                collect_exp(mut_vars, e);
            }
            N::SequenceItem_::Declare(sp!(_, lvalues), _) => {
                collect_lvalues(mut_vars, lvalues);
            }
            N::SequenceItem_::Seq(e) => collect_exp(mut_vars, e),
        }
    }
}

fn collect_lvalues(mut_vars: &mut BTreeMap<N::Var_, (Loc, Loc)>, lvalues: &[N::LValue]) {
    for sp!(_, lv_) in lvalues {
        match lv_ {
            N::LValue_::Var {
                mut_: Some(Mutability::Mut(mut_loc)),
                var: sp!(var_loc, var_),
                unused_binding: false,
            } if !var_.is_syntax_identifier()
                && !var_.starts_with_underscore()
                && var_.is_valid() =>
            {
                mut_vars.insert(*var_, (*var_loc, *mut_loc));
            }
            N::LValue_::Unpack(_, _, _, fields) => {
                for (_, _, (_, inner)) in fields {
                    collect_lvalues(mut_vars, std::slice::from_ref(inner));
                }
            }
            _ => (),
        }
    }
}

fn collect_exp(mut_vars: &mut BTreeMap<N::Var_, (Loc, Loc)>, sp!(_, e_): &N::Exp) {
    match e_ {
        N::Exp_::Block(N::Block { seq, .. }) => collect_seq(mut_vars, seq),
        N::Exp_::Lambda(N::Lambda {
            parameters: sp!(_, params),
            body,
            ..
        }) => {
            for (sp!(_, lvalues), _) in params {
                collect_lvalues(mut_vars, lvalues);
            }
            collect_exp(mut_vars, body);
        }
        N::Exp_::IfElse(econd, et, ef) => {
            collect_exp(mut_vars, econd);
            collect_exp(mut_vars, et);
            if let Some(ef) = ef {
                collect_exp(mut_vars, ef);
            }
        }
        N::Exp_::Match(esubject, arms) => {
            collect_exp(mut_vars, esubject);
            for arm in &arms.value {
                if let Some(guard) = &arm.value.guard {
                    collect_exp(mut_vars, guard);
                }
                collect_exp(mut_vars, &arm.value.rhs);
            }
        }
        N::Exp_::While(_, econd, ebody) => {
            collect_exp(mut_vars, econd);
            collect_exp(mut_vars, ebody);
        }
        _ => (),
    }
}

// ================================================================================================
// Checking: walk the body to find mutations, removing from mut_vars when found
// ================================================================================================

fn check_seq(mut_vars: &mut BTreeMap<N::Var_, (Loc, Loc)>, seq: &N::Sequence) {
    for sp!(_, item_) in &seq.1 {
        match item_ {
            N::SequenceItem_::Seq(e) | N::SequenceItem_::Bind(_, e) => {
                check_exp(mut_vars, e)
            }
            N::SequenceItem_::Declare(_, _) => (),
        }
    }
}

fn check_exp(mut_vars: &mut BTreeMap<N::Var_, (Loc, Loc)>, sp!(_, e_): &N::Exp) {
    if mut_vars.is_empty() {
        return;
    }
    match e_ {
        // Direct assignment: `x = e` or `(x, y) = e`
        N::Exp_::Assign(sp!(_, lvalues), rhs) => {
            for lv in lvalues {
                check_assign_lvalue(mut_vars, lv);
            }
            check_exp(mut_vars, rhs);
        }
        // Field mutation: `x.field = e`
        N::Exp_::FieldMutate(dotted, rhs) => {
            check_dotted_base(mut_vars, dotted);
            check_exp(mut_vars, rhs);
        }
        // Dereference mutation: `*x = e`
        N::Exp_::Mutate(lhs, rhs) => {
            check_exp(mut_vars, lhs);
            check_exp(mut_vars, rhs);
        }
        // Mutable borrow: `&mut x` or `&mut x.field`
        N::Exp_::ExpDotted(E::DottedUsage::Borrow(true), dotted) => {
            check_dotted_base(mut_vars, dotted);
        }
        // Recurse into subexpressions
        N::Exp_::Block(N::Block { seq, .. }) => check_seq(mut_vars, seq),
        N::Exp_::Lambda(N::Lambda { body, .. }) => check_exp(mut_vars, body),
        N::Exp_::IfElse(econd, et, ef) => {
            check_exp(mut_vars, econd);
            check_exp(mut_vars, et);
            if let Some(ef) = ef {
                check_exp(mut_vars, ef);
            }
        }
        N::Exp_::Match(esubject, arms) => {
            check_exp(mut_vars, esubject);
            for arm in &arms.value {
                if let Some(guard) = &arm.value.guard {
                    check_exp(mut_vars, guard);
                }
                check_exp(mut_vars, &arm.value.rhs);
            }
        }
        N::Exp_::While(_, econd, ebody) | N::Exp_::BinopExp(econd, _, ebody) => {
            check_exp(mut_vars, econd);
            check_exp(mut_vars, ebody);
        }
        N::Exp_::Loop(_, e)
        | N::Exp_::Return(e)
        | N::Exp_::Abort(e)
        | N::Exp_::Give(_, _, e)
        | N::Exp_::Dereference(e)
        | N::Exp_::UnaryExp(_, e)
        | N::Exp_::Cast(e, _)
        | N::Exp_::Annotate(e, _) => check_exp(mut_vars, e),
        N::Exp_::ModuleCall(_, _, _, _, sp!(_, es))
        | N::Exp_::VarCall(_, sp!(_, es))
        | N::Exp_::Builtin(_, sp!(_, es))
        | N::Exp_::Vector(_, _, sp!(_, es))
        | N::Exp_::ExpList(es) => {
            for e in es {
                check_exp(mut_vars, e);
            }
        }
        // Method calls: the receiver might be `&mut self`, so conservatively treat it as
        // mutating. We don't have type info at the naming level to know otherwise.
        N::Exp_::MethodCall(dotted, _, _, _, _, sp!(_, es)) => {
            check_dotted_base(mut_vars, dotted);
            for e in es {
                check_exp(mut_vars, e);
            }
        }
        N::Exp_::Pack(_, _, _, fields) | N::Exp_::PackVariant(_, _, _, _, fields) => {
            for (_, _, (_, e)) in fields {
                check_exp(mut_vars, e);
            }
        }
        N::Exp_::ExpDotted(_, dotted) => check_exp_dotted(mut_vars, dotted),
        N::Exp_::Value(_)
        | N::Exp_::Var(_)
        | N::Exp_::Constant(_, _)
        | N::Exp_::Continue(_)
        | N::Exp_::Unit { .. }
        | N::Exp_::ErrorConstant { .. }
        | N::Exp_::UnresolvedError => (),
    }
}

/// Check if an lvalue in an assignment is one of our tracked mut variables.
fn check_assign_lvalue(
    mut_vars: &mut BTreeMap<N::Var_, (Loc, Loc)>,
    sp!(_, lv_): &N::LValue,
) {
    match lv_ {
        N::LValue_::Var { var: sp!(_, v), .. } => {
            mut_vars.remove(v);
        }
        N::LValue_::Unpack(_, _, _, fields) => {
            for (_, _, (_, inner)) in fields {
                check_assign_lvalue(mut_vars, inner);
            }
        }
        N::LValue_::Ignore | N::LValue_::Error => (),
    }
}

/// Find the base variable of a dotted expression and mark it as mutated.
fn check_dotted_base(
    mut_vars: &mut BTreeMap<N::Var_, (Loc, Loc)>,
    sp!(_, dotted_): &N::ExpDotted,
) {
    match dotted_ {
        N::ExpDotted_::Exp(e) => {
            if let N::Exp_::Var(sp!(_, v)) = &e.value {
                mut_vars.remove(v);
            }
        }
        N::ExpDotted_::Dot(inner, _, _) | N::ExpDotted_::DotAutocomplete(_, inner) => {
            check_dotted_base(mut_vars, inner);
        }
        N::ExpDotted_::Index(inner, _) => {
            check_dotted_base(mut_vars, inner);
        }
    }
}

/// Walk a dotted expression recursively (for non-mutation contexts).
fn check_exp_dotted(
    mut_vars: &mut BTreeMap<N::Var_, (Loc, Loc)>,
    sp!(_, dotted_): &N::ExpDotted,
) {
    match dotted_ {
        N::ExpDotted_::Exp(e) => check_exp(mut_vars, e),
        N::ExpDotted_::Dot(inner, _, _) | N::ExpDotted_::DotAutocomplete(_, inner) => {
            check_exp_dotted(mut_vars, inner)
        }
        N::ExpDotted_::Index(inner, sp!(_, es)) => {
            check_exp_dotted(mut_vars, inner);
            for e in es {
                check_exp(mut_vars, e);
            }
        }
    }
}
