// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{cfg::MutForwardCFG, remove_no_ops},
    diagnostics::DiagnosticReporter,
    expansion::ast::Mutability,
    hlir::ast::{FunctionSignature, SingleType, Value, Var},
    parser,
    shared::unique_map::UniqueMap,
};
use std::collections::BTreeSet;

/// returns true if anything changed
pub fn optimize(
    _reporter: &DiagnosticReporter,
    signature: &FunctionSignature,
    _locals: &UniqueMap<Var, (Mutability, SingleType)>,
    _constants: &UniqueMap<parser::ast::ConstantName, Value>,
    cfg: &mut MutForwardCFG,
) -> bool {
    let changed = remove_no_ops::optimize(cfg);
    let ssa_temps = {
        let s = count(signature, cfg);
        if s.is_empty() {
            return changed;
        }
        s
    };

    // `eliminate` always removes if `ssa_temps` is not empty
    eliminate(cfg, ssa_temps);
    remove_no_ops::optimize(cfg);
    true
}

//**************************************************************************************************
// Count assignment and usage
//**************************************************************************************************

fn count(signature: &FunctionSignature, cfg: &MutForwardCFG) -> BTreeSet<Var> {
    let mut context = count::Context::new(signature);
    for block in cfg.blocks().values() {
        for cmd in block {
            count::command(&mut context, cmd)
        }
    }
    context.finish()
}

mod count {
    use move_proc_macros::growing_stack;

    use crate::{
        hlir::ast::{FunctionSignature, *},
        parser::ast::{BinOp, UnaryOp},
    };
    use std::collections::{BTreeMap, BTreeSet};

    pub struct Context {
        assigned: BTreeMap<Var, Option<usize>>,
        used: BTreeMap<Var, Option<usize>>,
    }

    impl Context {
        pub fn new(signature: &FunctionSignature) -> Self {
            let mut ctx = Context {
                assigned: BTreeMap::new(),
                used: BTreeMap::new(),
            };
            for (_, v, _) in &signature.parameters {
                ctx.assign(v, false);
            }
            ctx
        }

        fn assign(&mut self, var: &Var, substitutable: bool) {
            if !substitutable {
                self.assigned.insert(*var, None);
                return;
            }

            if let Some(count) = self.assigned.entry(*var).or_insert_with(|| Some(0)) {
                *count += 1
            }
        }

        fn used(&mut self, var: &Var, substitutable: bool) {
            if !substitutable {
                self.used.insert(*var, None);
                return;
            }

            if let Some(count) = self.used.entry(*var).or_insert_with(|| Some(0)) {
                *count += 1
            }
        }

        pub fn finish(self) -> BTreeSet<Var> {
            let Context { assigned, used } = self;
            assigned
                .into_iter()
                .filter(|(_v, count)| count.map(|c| c == 1).unwrap_or(false))
                .map(|(v, _count)| v)
                .filter(|v| {
                    used.get(v)
                        .unwrap_or(&None)
                        .map(|c| c == 1)
                        .unwrap_or(false)
                })
                .collect()
        }
    }

    #[growing_stack]
    pub fn command(context: &mut Context, sp!(_, cmd_): &Command) {
        use Command_ as C;
        match cmd_ {
            C::Assign(_, ls, e) => {
                exp(context, e);
                let substitutable_rvalues = can_subst_exp(ls.len(), e);
                lvalues(context, ls, substitutable_rvalues);
            }
            C::Mutate(el, er) => {
                exp(context, er);
                exp(context, el)
            }
            C::Return { exp: e, .. }
            | C::Abort(_, e)
            | C::IgnoreAndPop { exp: e, .. }
            | C::JumpIf { cond: e, .. }
            | C::VariantSwitch { subject: e, .. } => exp(context, e),

            C::Jump { .. } => (),
            C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
        }
    }

    fn lvalues(context: &mut Context, ls: &[LValue], substitutable_rvalues: Vec<bool>) {
        assert!(ls.len() == substitutable_rvalues.len());
        ls.iter()
            .zip(substitutable_rvalues)
            .for_each(|(l, substitutable)| lvalue(context, l, substitutable))
    }

    fn lvalue(context: &mut Context, sp!(_, l_): &LValue, substitutable: bool) {
        use LValue_ as L;
        match l_ {
            L::Ignore | L::Unpack(_, _, _) | L::UnpackVariant(..) => (),
            L::Var { var, .. } => context.assign(var, substitutable),
        }
    }

    #[growing_stack]
    fn exp(context: &mut Context, parent_e: &Exp) {
        use UnannotatedExp_ as E;
        match &parent_e.exp.value {
            E::Unit { .. }
            | E::Value(_)
            | E::Constant(_)
            | E::UnresolvedError
            | E::ErrorConstant { .. } => (),

            E::BorrowLocal(_, var) => context.used(var, false),

            E::Copy { var, .. } | E::Move { var, .. } => context.used(var, true),

            E::ModuleCall(mcall) => {
                for arg in &mcall.arguments {
                    exp(context, arg);
                }
            }
            E::Vector(_, _, _, args) => {
                for arg in args.iter() {
                    exp(context, arg);
                }
            }

            E::Freeze(e)
            | E::Dereference(e)
            | E::UnaryExp(_, e)
            | E::Borrow(_, e, _, _)
            | E::Cast(e, _) => exp(context, e),

            E::BinopExp(e1, _, e2) => {
                exp(context, e1);
                exp(context, e2)
            }

            E::Pack(_, _, fields) => fields.iter().for_each(|(_, _, e)| exp(context, e)),

            E::PackVariant(_, _, _, fields) => fields.iter().for_each(|(_, _, e)| exp(context, e)),

            E::Multiple(es) => es.iter().for_each(|e| exp(context, e)),

            E::Unreachable => panic!("ICE should not analyze dead code"),
        }
    }

    fn can_subst_exp(lvalue_len: usize, exp: &Exp) -> Vec<bool> {
        use UnannotatedExp_ as E;
        match (lvalue_len, &exp.exp.value) {
            (0, _) => vec![],
            (1, _) => vec![can_subst_exp_single(exp)],
            (_, E::Multiple(es)) => es.iter().map(can_subst_exp_single).collect(),
            (_, _) => (0..lvalue_len).map(|_| false).collect(),
        }
    }

    fn can_subst_exp_single(parent_e: &Exp) -> bool {
        use UnannotatedExp_ as E;
        match &parent_e.exp.value {
            E::UnresolvedError
            | E::ErrorConstant { .. }
            | E::BorrowLocal(_, _)
            | E::Copy { .. }
            | E::Freeze(_)
            | E::Dereference(_)
            | E::ModuleCall(_)
            | E::Move { .. }
            | E::Borrow(_, _, _, _) => false,

            E::Unit { .. } | E::Value(_) | E::Constant(_) => true,

            E::Cast(e, _) => can_subst_exp_single(e),
            E::UnaryExp(op, e) => can_subst_exp_unary(op) && can_subst_exp_single(e),
            E::BinopExp(e1, op, e2) => {
                can_subst_exp_binary(op) && can_subst_exp_single(e1) && can_subst_exp_single(e2)
            }
            E::Multiple(es) => es.iter().all(can_subst_exp_single),
            E::Pack(_, _, fields) => fields.iter().all(|(_, _, e)| can_subst_exp_single(e)),
            E::PackVariant(_, _, _, fields) => {
                fields.iter().all(|(_, _, e)| can_subst_exp_single(e))
            }
            E::Vector(_, _, _, eargs) => eargs.iter().all(can_subst_exp_single),

            E::Unreachable => panic!("ICE should not analyze dead code"),
        }
    }

    fn can_subst_exp_unary(sp!(_, op_): &UnaryOp) -> bool {
        op_.is_pure()
    }

    fn can_subst_exp_binary(sp!(_, op_): &BinOp) -> bool {
        op_.is_pure()
    }
}

//**************************************************************************************************
// Eliminate
//**************************************************************************************************

fn eliminate(cfg: &mut MutForwardCFG, ssa_temps: BTreeSet<Var>) {
    let context = &mut eliminate::Context::new(ssa_temps);
    loop {
        for block in cfg.blocks_mut().values_mut() {
            for cmd in block {
                eliminate::command(context, cmd)
            }
        }
        if context.finished() {
            return;
        }
    }
}

mod eliminate {
    use crate::hlir::ast::{self as H, *};
    use move_ir_types::location::*;
    use move_proc_macros::growing_stack;
    use std::collections::{BTreeMap, BTreeSet};

    pub struct Context {
        eliminated: BTreeMap<Var, Exp>,
        ssa_temps: BTreeSet<Var>,
    }

    impl Context {
        pub fn new(ssa_temps: BTreeSet<Var>) -> Self {
            Context {
                ssa_temps,
                eliminated: BTreeMap::new(),
            }
        }

        pub fn finished(&self) -> bool {
            self.eliminated.is_empty() && self.ssa_temps.is_empty()
        }
    }

    #[growing_stack]
    pub fn command(context: &mut Context, sp!(_, cmd_): &mut Command) {
        use Command_ as C;
        match cmd_ {
            C::Assign(_, ls, e) => {
                exp(context, e);
                let eliminated = lvalues(context, ls);
                remove_eliminated(context, eliminated, e)
            }
            C::Mutate(el, er) => {
                exp(context, er);
                exp(context, el)
            }
            C::Return { exp: e, .. }
            | C::Abort(_, e)
            | C::IgnoreAndPop { exp: e, .. }
            | C::JumpIf { cond: e, .. }
            | C::VariantSwitch { subject: e, .. } => exp(context, e),

            C::Jump { .. } => (),
            C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
        }
    }

    enum LRes {
        Same(LValue),
        Elim(Var),
    }

    fn lvalues(context: &mut Context, ls: &mut Vec<LValue>) -> Vec<Option<Var>> {
        let old = std::mem::take(ls);
        old.into_iter()
            .map(|l| match lvalue(context, l) {
                LRes::Same(lvalue) => {
                    ls.push(lvalue);
                    None
                }
                LRes::Elim(v) => Some(v),
            })
            .collect()
    }

    fn lvalue(context: &mut Context, sp!(loc, l_): LValue) -> LRes {
        use LValue_ as L;
        match l_ {
            l_ @ (L::Ignore | L::Unpack(_, _, _) | L::UnpackVariant(..)) => LRes::Same(sp(loc, l_)),
            L::Var {
                var,
                ty,
                unused_assignment,
            } => {
                let contained = context.ssa_temps.remove(&var);
                if contained {
                    LRes::Elim(var)
                } else {
                    LRes::Same(sp(
                        loc,
                        L::Var {
                            var,
                            ty,
                            unused_assignment,
                        },
                    ))
                }
            }
        }
    }

    #[growing_stack]
    fn exp(context: &mut Context, parent_e: &mut Exp) {
        use UnannotatedExp_ as E;
        match &mut parent_e.exp.value {
            E::Copy { var, .. } | E::Move { var, .. } => {
                if let Some(replacement) = context.eliminated.remove(var) {
                    *parent_e = replacement
                }
            }

            E::Unit { .. }
            | E::Value(_)
            | E::Constant(_)
            | E::UnresolvedError
            | E::ErrorConstant { .. }
            | E::BorrowLocal(_, _) => (),

            E::ModuleCall(mcall) => {
                for arg in mcall.arguments.iter_mut() {
                    exp(context, arg);
                }
            }
            E::Vector(_, _, _, args) => {
                for arg in args.iter_mut() {
                    exp(context, arg);
                }
            }
            E::Freeze(e)
            | E::Dereference(e)
            | E::UnaryExp(_, e)
            | E::Borrow(_, e, _, _)
            | E::Cast(e, _) => exp(context, e),

            E::BinopExp(e1, _, e2) => {
                exp(context, e1);
                exp(context, e2)
            }

            E::Pack(_, _, fields) => fields.iter_mut().for_each(|(_, _, e)| exp(context, e)),

            E::PackVariant(_, _, _, fields) => {
                fields.iter_mut().for_each(|(_, _, e)| exp(context, e))
            }

            E::Multiple(es) => es.iter_mut().for_each(|e| exp(context, e)),

            E::Unreachable => panic!("ICE should not analyze dead code"),
        }
    }

    fn remove_eliminated(context: &mut Context, mut eliminated: Vec<Option<Var>>, e: &mut Exp) {
        if eliminated.iter().all(|opt| opt.is_none()) {
            return;
        }

        match eliminated.len() {
            0 => (),
            1 => remove_eliminated_single(context, eliminated.pop().unwrap().unwrap(), e),
            _ => {
                let tys = match &mut e.ty.value {
                    Type_::Multiple(tys) => tys,
                    _ => panic!("ICE local elimination type mismatch"),
                };
                let es = match &mut e.exp.value {
                    UnannotatedExp_::Multiple(es) => es,
                    _ => panic!("ICE local elimination type mismatch"),
                };
                let old_tys = std::mem::take(tys);
                let old_es = std::mem::take(es);
                for ((mut e, ty), elim_opt) in old_es.into_iter().zip(old_tys).zip(eliminated) {
                    match elim_opt {
                        None => {
                            tys.push(ty);
                            es.push(e)
                        }
                        Some(v) => {
                            remove_eliminated_single(context, v, &mut e);
                            match &e.ty.value {
                                Type_::Unit => (),
                                Type_::Single(_) => {
                                    tys.push(ty);
                                    es.push(e)
                                }
                                Type_::Multiple(_) => {
                                    panic!("ICE local elimination replacement type mismatch")
                                }
                            }
                        }
                    }
                }
                if es.is_empty() {
                    *e = unit(e.exp.loc)
                }
            }
        }
    }

    fn remove_eliminated_single(context: &mut Context, v: Var, e: &mut Exp) {
        let old = std::mem::replace(e, unit(e.exp.loc));
        context.eliminated.insert(v, old);
    }

    fn unit(loc: Loc) -> Exp {
        H::exp(
            sp(loc, Type_::Unit),
            sp(
                loc,
                UnannotatedExp_::Unit {
                    case: UnitCase::Implicit,
                },
            ),
        )
    }
}
