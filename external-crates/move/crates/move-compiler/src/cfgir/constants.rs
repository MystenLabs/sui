// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rewrites cross-module constant references in function bodies into calls of the
//! compiler-generated `public(package)` function synthesized in the defining module.
//! References in constant definitions do not go through this: they are resolved by constant
//! folding.
// TODO(cross-module-constants): this is one of several hand-rolled HLIR/typed-AST walkers
// (`dependent_constants` and the dependency-ordering walker). There is no mutable HLIR visitor
// today; consider a shared walker if another one appears.

use super::translate::Context;
use crate::{expansion::ast::ModuleIdent, hlir::ast as H, ice};
use move_proc_macros::growing_stack;

pub(super) fn rewrite_constant_calls(
    context: &mut Context,
    module: ModuleIdent,
    body: &mut H::Block,
) {
    if context.constant_fns.is_empty() {
        return;
    }
    block(context, module, body)
}

fn block(context: &mut Context, module: ModuleIdent, block: &mut H::Block) {
    for stmt in block {
        statement(context, module, stmt);
    }
}

fn statement(context: &mut Context, module: ModuleIdent, sp!(_, stmt_): &mut H::Statement) {
    use H::Statement_ as S;
    match stmt_ {
        S::Command(cmd) => command(context, module, cmd),
        S::IfElse {
            cond,
            if_block,
            else_block,
        } => {
            exp(context, module, cond);
            block(context, module, if_block);
            block(context, module, else_block)
        }
        S::VariantMatch {
            subject,
            enum_name: _,
            arms,
        } => {
            exp(context, module, subject);
            for (_, arm) in arms {
                block(context, module, arm);
            }
        }
        S::While {
            cond: (cond_block, cond_exp),
            block: body,
            ..
        } => {
            block(context, module, cond_block);
            exp(context, module, cond_exp);
            block(context, module, body)
        }
        S::Loop { block: body, .. } => block(context, module, body),
        S::NamedBlock { block: body, .. } => block(context, module, body),
    }
}

fn command(context: &mut Context, module: ModuleIdent, sp!(_, cmd_): &mut H::Command) {
    use H::Command_ as C;
    match cmd_ {
        C::IgnoreAndPop { exp: e, .. }
        | C::Return { exp: e, .. }
        | C::Abort(_, e)
        | C::Assign(_, _, e)
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. } => exp(context, module, e),
        C::Mutate(lhs, rhs) => {
            exp(context, module, lhs);
            exp(context, module, rhs)
        }
        C::Break(_) | C::Continue(_) | C::Jump { .. } => (),
    }
}

#[growing_stack]
fn exp(context: &mut Context, module: ModuleIdent, e: &mut H::Exp) {
    use H::UnannotatedExp_ as E;
    let eloc = e.exp.loc;
    match &mut e.exp.value {
        e_ @ E::Constant(_, _) => {
            let E::Constant(m, c) = e_ else {
                unreachable!()
            };
            if *m == module {
                return;
            }
            if let Some(constant_fn) = context.constant_fns.get(&(*m, *c)).copied() {
                *e_ = E::ModuleCall(Box::new(H::ModuleCall {
                    module: *m,
                    name: constant_fn,
                    type_arguments: vec![],
                    arguments: vec![],
                }));
            } else {
                // reachable only when typing rejected the access (private constant, feature
                // off, or cross-package)
                if !context.env.has_errors() {
                    context.add_diag(ice!((
                        eloc,
                        "cross-module constant use with no generated constant function"
                    )));
                }
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
                exp(context, module, arg);
            }
        }
        E::Freeze(base) | E::Dereference(base) | E::UnaryExp(_, base) | E::Cast(base, _) => {
            exp(context, module, base)
        }
        E::Borrow(_, base, _, _) => exp(context, module, base),
        E::BinopExp(lhs, _, rhs) => {
            exp(context, module, lhs);
            exp(context, module, rhs)
        }
        E::Pack(_, _, fields) | E::PackVariant(_, _, _, fields) => {
            for (_, _, fe) in fields {
                exp(context, module, fe);
            }
        }
        E::Vector(_, _, _, args) | E::Multiple(args) => {
            for arg in args {
                exp(context, module, arg);
            }
        }
    }
}
