// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rewrites cross-module constant references in function bodies into references to module-local
//! copies of the referenced constants, synthesized from their already-folded values (see
//! `Context::constant_copy`). References in constant definitions do not go through this: they are
//! resolved by constant folding.
// TODO(cross-module-constants): this is one of several hand-rolled HLIR/typed-AST walkers
// (`compute_dependent_constants` and the dependency-ordering walker). There is no mutable HLIR
// visitor today; consider a shared walker if another one appears.

use crate::{
    cfgir::translate::Context, expansion::ast::ModuleIdent, hlir::ast as H, ice_assert,
    parser::ast::ConstantName,
};
use move_proc_macros::growing_stack;
use std::collections::{BTreeMap, BTreeSet};

type ConstantValues = BTreeMap<(ModuleIdent, ConstantName), H::Value>;

pub(super) fn rewrite_cross_module_constants(
    context: &mut Context,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    body: &mut H::Block,
) {
    block(context, constant_values, module, body)
}

fn block(
    context: &mut Context,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    block: &mut H::Block,
) {
    for stmt in block {
        statement(context, constant_values, module, stmt);
    }
}

fn statement(
    context: &mut Context,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    sp!(_, stmt_): &mut H::Statement,
) {
    use H::Statement_ as S;
    match stmt_ {
        S::Command(cmd) => command(context, constant_values, module, cmd),
        S::IfElse {
            cond,
            if_block,
            else_block,
        } => {
            exp(context, constant_values, module, cond);
            block(context, constant_values, module, if_block);
            block(context, constant_values, module, else_block)
        }
        S::VariantMatch {
            subject,
            enum_name: _,
            arms,
        } => {
            exp(context, constant_values, module, subject);
            for (_, arm) in arms {
                block(context, constant_values, module, arm);
            }
        }
        S::While {
            cond: (cond_block, cond_exp),
            block: body,
            ..
        } => {
            block(context, constant_values, module, cond_block);
            exp(context, constant_values, module, cond_exp);
            block(context, constant_values, module, body)
        }
        S::Loop { block: body, .. } => block(context, constant_values, module, body),
        S::NamedBlock { block: body, .. } => block(context, constant_values, module, body),
    }
}

fn command(
    context: &mut Context,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    sp!(_, cmd_): &mut H::Command,
) {
    use H::Command_ as C;
    match cmd_ {
        C::IgnoreAndPop { exp: e, .. }
        | C::Return { exp: e, .. }
        | C::Abort(_, e)
        | C::Assign(_, _, e)
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. } => exp(context, constant_values, module, e),
        C::Mutate(lhs, rhs) => {
            exp(context, constant_values, module, lhs);
            exp(context, constant_values, module, rhs)
        }
        C::Break(_) | C::Continue(_) | C::Jump { .. } => (),
    }
}

#[growing_stack]
fn exp(
    context: &mut Context,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    e: &mut H::Exp,
) {
    use H::UnannotatedExp_ as E;
    let eloc = e.exp.loc;
    match &mut e.exp.value {
        e_ @ E::Constant(_, _) => {
            let E::Constant(m, c) = e_ else {
                unreachable!()
            };
            let (m, c) = (*m, *c);
            if m == module {
                return;
            }
            if let Some(copy_name) = context.constant_copy(constant_values, m, c, eloc) {
                *e_ = E::Constant(module, copy_name);
            } else {
                // an error was reported either at the constant's definition or at this use
                ice_assert!(
                    context.reporter(),
                    context.env.has_errors(),
                    eloc,
                    "cross-module constant use failed to resolve to a module-local copy"
                );
                *e_ = E::UnresolvedError;
            }
        }

        E::Unit { .. }
        | E::Value(_)
        | E::Move { .. }
        | E::Copy { .. }
        | E::ErrorConstant { .. }
        | E::BorrowLocal(_, _)
        | E::UnresolvedError
        | E::Unreachable => (),

        E::ModuleCall(mcall) => {
            for arg in &mut mcall.arguments {
                exp(context, constant_values, module, arg);
            }
        }
        E::Freeze(base) | E::Dereference(base) | E::UnaryExp(_, base) | E::Cast(base, _) => {
            exp(context, constant_values, module, base)
        }
        E::Borrow(_, base, _, _) => exp(context, constant_values, module, base),
        E::BinopExp(lhs, _, rhs) => {
            exp(context, constant_values, module, lhs);
            exp(context, constant_values, module, rhs)
        }
        E::Pack(_, _, fields) | E::PackVariant(_, _, _, fields) => {
            for (_, _, fe) in fields {
                exp(context, constant_values, module, fe);
            }
        }
        E::Vector(_, _, _, args) | E::Multiple(args) => {
            for arg in args {
                exp(context, constant_values, module, arg);
            }
        }
    }
}

//**************************************************************************************************
// Constant dependencies
//**************************************************************************************************

pub(super) fn compute_dependent_constants(
    constant: &H::Constant,
) -> BTreeSet<(ModuleIdent, ConstantName)> {
    fn dep_exp(set: &mut BTreeSet<(ModuleIdent, ConstantName)>, exp: &H::Exp) {
        use H::UnannotatedExp_ as E;
        match &exp.exp.value {
            E::UnresolvedError
            | E::Unreachable
            | E::Unit { .. }
            | E::Value(_)
            | E::Move { .. }
            | E::Copy { .. } => (),
            E::UnaryExp(_, rhs) => dep_exp(set, rhs),
            E::BinopExp(lhs, _, rhs) => {
                dep_exp(set, lhs);
                dep_exp(set, rhs)
            }
            E::Cast(base, _) => dep_exp(set, base),
            E::Vector(_, _, _, args) | E::Multiple(args) => {
                for arg in args {
                    dep_exp(set, arg);
                }
            }
            E::Constant(m, c) => {
                set.insert((*m, *c));
            }
            _ => panic!("ICE typing should have rejected exp in const"),
        }
    }

    fn dep_cmd(set: &mut BTreeSet<(ModuleIdent, ConstantName)>, command: &H::Command_) {
        use H::Command_ as C;
        match command {
            C::IgnoreAndPop { exp, .. } => dep_exp(set, exp),
            C::Return { exp, .. } => dep_exp(set, exp),
            C::Abort(_, exp) | C::Assign(_, _, exp) => dep_exp(set, exp),
            C::Mutate(lhs, rhs) => {
                dep_exp(set, lhs);
                dep_exp(set, rhs)
            }
            C::Break(_)
            | C::Continue(_)
            | C::Jump { .. }
            | C::JumpIf { .. }
            | C::VariantSwitch { .. } => (),
        }
    }

    fn dep_stmt(set: &mut BTreeSet<(ModuleIdent, ConstantName)>, stmt: &H::Statement_) {
        use H::Statement_ as S;
        match stmt {
            S::Command(cmd) => dep_cmd(set, &cmd.value),
            S::IfElse {
                cond,
                if_block,
                else_block,
            } => {
                dep_exp(set, cond);
                dep_block(set, if_block);
                dep_block(set, else_block)
            }
            S::VariantMatch {
                subject,
                enum_name: _,
                arms,
            } => {
                dep_exp(set, subject);
                for (_, arm) in arms {
                    dep_block(set, arm);
                }
            }
            S::While {
                cond: (cond_block, cond_exp),
                block,
                ..
            } => {
                dep_block(set, cond_block);
                dep_exp(set, cond_exp);
                dep_block(set, block)
            }
            S::Loop { block, .. } => dep_block(set, block),
            S::NamedBlock { block, .. } => dep_block(set, block),
        }
    }

    fn dep_block(set: &mut BTreeSet<(ModuleIdent, ConstantName)>, block: &H::Block) {
        for entry in block {
            dep_stmt(set, &entry.value);
        }
    }

    let mut output = BTreeSet::new();
    let (_, block) = &constant.value;
    dep_block(&mut output, block);
    output
}
