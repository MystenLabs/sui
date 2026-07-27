// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rewrites cross-module constant references in function bodies into references to module-local
//! copies of the referenced constants: when module `a` uses `b::C`, the already-folded value of
//! `b::C` is synthesized into `a` as a new constant (see [`synthesize_copy`]) and the use is
//! rewritten to refer to that copy. The value is copied, never re-folded, so a constant whose
//! definition failed to fold reports an error at each cross-module use instead. References in
//! constant definitions do not go through this: they are resolved by constant folding.
// TODO(cross-module-constants): this is one of several hand-rolled HLIR/typed-AST walkers
// (`dependent_constants` and the dependency-ordering walker). There is no mutable HLIR visitor
// today; consider a shared walker if another one appears.

use super::translate::{ConstantEntry, Context, move_value_from_value};
use crate::{
    cfgir::ast as G,
    diag,
    diagnostics::{Diagnostic, filter::empty_filter_scope},
    expansion::ast::ModuleIdent,
    hlir::ast as H,
    ice,
    parser::ast::ConstantName,
    shared::unique_map::UniqueMap,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use std::collections::BTreeMap;

type ConstantValues = BTreeMap<(ModuleIdent, ConstantName), H::Value>;

pub(super) fn rewrite_cross_module_constants(
    context: &mut Context,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    body: &mut H::Block,
) {
    block(context, constant_values, module, body)
}

/// The error reported at each use of a constant whose definition could not be evaluated to a
/// value. Also used for cross-module references in constant definitions (see
/// `translate::check_constant_value`).
pub(super) fn unfoldable_constant_use(
    m: &ModuleIdent,
    c: &ConstantName,
    use_loc: Loc,
    defined_loc: Loc,
) -> Diagnostic {
    let msg = format!(
        "Invalid use of constant '{}::{}'. Its value could not be computed",
        m, c
    );
    let mut diag = diag!(CodeGeneration::UnfoldableConstant, (use_loc, msg));
    diag.add_secondary_label((defined_loc, format!("'{}' is defined here", c)));
    diag
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
            if let Some(copy_name) = context.constant_copies.get(&(m, c)) {
                *e_ = E::Constant(module, copy_name);
                return;
            }
            match context.constant_defs.get_key_value(&(m, c)) {
                Some((_, ConstantEntry::Defined { signature })) => {
                    let signature = (**signature).clone();
                    let copy_name =
                        synthesize_copy(context, constant_values, m, c, signature, eloc);
                    *e_ = E::Constant(module, copy_name);
                }
                Some(((_, defined), ConstantEntry::Failed)) => {
                    let defined_loc = defined.0.loc;
                    context.add_diag(unfoldable_constant_use(&m, &c, eloc, defined_loc));
                    *e_ = E::UnresolvedError;
                }
                Some((_, ConstantEntry::Pending)) => {
                    // The defining module has not been processed yet, which is only possible under
                    // a module dependency cycle, already reported during typing
                    if !context.env.has_errors() {
                        context.add_diag(ice!((
                            eloc,
                            "cross-module constant use before its defining module was processed"
                        )));
                    }
                    *e_ = E::UnresolvedError;
                }
                None => {
                    // TODO(cross-module-constants): constants in pre-compiled dependencies have
                    // their folded values in `PreCompiledProgramInfo`; supporting them is a
                    // possible follow-up.
                    let msg = format!(
                        "Invalid access of '{}::{}'. Constants defined in modules outside of the \
                         current compilation cannot be accessed from other modules",
                        m, c
                    );
                    context.add_diag(diag!(TypeSafety::Visibility, (eloc, msg)));
                    *e_ = E::UnresolvedError;
                }
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

/// Synthesizes a module-local copy of the constant `m::c` from its already-folded value, named
/// `_{dep_order}_{module}_{const}`, e.g. `b::C` with `b` at dependency order 3 becomes `_3_b_C`.
/// The leading `_` means the name can never collide with a user constant (those must start with an
/// uppercase letter). Leading with the defining module's dependency order — unique per module —
/// means two distinct source constants can never mangle to the same name: equal names have equal
/// leading digit runs, hence the same defining module, hence the same constant name (unique within
/// their module). Note these names are not stable across module-graph changes (dependency orders
/// renumber); they appear only in source maps and are not part of the upgrade-compatibility
/// surface.
fn synthesize_copy(
    context: &mut Context,
    constant_values: &ConstantValues,
    m: ModuleIdent,
    c: ConstantName,
    signature: H::BaseType,
    first_use: Loc,
) -> ConstantName {
    let value = constant_values
        .get(&(m, c))
        .expect("ICE defined constant with no folded value");
    let dep_order = context
        .info
        .module(&m)
        .dependency_order
        .expect("ICE typed module with no dependency order");
    let symbol = format!("_{}_{}_{}", dep_order, m.value.module, c).into();
    // The first-use loc (rather than the defining loc) keeps every loc in the using module's
    // source map within its own file
    let copy_name = ConstantName(sp(first_use, symbol));
    let cdef = G::Constant {
        warning_filter: empty_filter_scope(),
        index: context.constant_copies.next_index(),
        attributes: UniqueMap::new(),
        loc: first_use,
        signature,
        value: Some(move_value_from_value(value.clone())),
    };
    if !context.constant_copies.add(m, c, copy_name, cdef) {
        context.add_diag(ice!((
            first_use,
            format!("mangled constant copy name collision on '{}'", copy_name)
        )));
    }
    copy_name
}
