// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rewrites a module's cross-module constant uses into uses of module-local copies, synthesized
//! on demand from the values `folding` computed.

// TODO(cross-module-constants): the walker below is a hand-rolled mutable HLIR walker; there is no
// mutable HLIR visitor today. Consider a shared walker if another one appears.

use crate::{
    cfgir::{
        ast as G,
        constants::{ConstantEntry, ConstantId, Constants, unfoldable_constant_use_error},
        translate::{Context as CfgirContext, move_value_from_value},
    },
    diagnostics::{Diagnostic, DiagnosticReporter, filter::empty_filter_scope},
    expansion::ast::{Address, ModuleIdent},
    hlir::{ast as H, translate::NEW_NAME_DELIM},
    ice, ice_assert,
    parser::ast::{ConstantName, FunctionName},
    shared::unique_map::UniqueMap,
};

use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;

use std::collections::BTreeMap;

//**************************************************************************************************
// Context
//**************************************************************************************************

struct Context<'ctx, 'env> {
    inner_context: &'ctx CfgirContext<'env>,
    constants: &'ctx Constants,
    module: ModuleIdent,
    copies: BTreeMap<ConstantId, ConstantName>,
    copy_defs: Vec<(ConstantName, G::Constant)>,
    /// Continues after the module's own constants so copies are emitted after them.
    /// Computed as `max(index) + 1` rather than `len()`, because failed constants
    /// are absent from this map.
    next_index: usize,
}

impl<'ctx, 'env> Context<'ctx, 'env> {
    fn new(
        context: &'ctx CfgirContext<'env>,
        constants: &'ctx Constants,
        module: ModuleIdent,
        module_constants: &UniqueMap<ConstantName, G::Constant>,
    ) -> Self {
        Self {
            inner_context: context,
            constants,
            module,
            copies: BTreeMap::new(),
            copy_defs: vec![],
            next_index: module_constants
                .key_cloned_iter()
                .map(|(_, cdef)| cdef.index + 1)
                .max()
                .unwrap_or(0),
        }
    }

    /// The copies synthesized for the module, to be added to its constants.
    fn into_copies(self) -> Vec<(ConstantName, G::Constant)> {
        self.copy_defs
    }

    fn reporter(&self) -> &DiagnosticReporter<'env> {
        self.inner_context.reporter()
    }

    fn add_diag(&self, diag: Diagnostic) {
        self.inner_context.add_diag(diag);
    }

    fn has_errors(&self) -> bool {
        self.inner_context.env.has_errors()
    }

    /// Resolves a cross-module constant use to a module-local copy of the constant, synthesizing
    /// the copy at its first use. Returns `None` if no copy can be made (e.g., the constant failed
    /// during folding, etc).
    fn constant_copy(
        &mut self,
        m: ModuleIdent,
        c: ConstantName,
        use_loc: Loc,
    ) -> Option<ConstantName> {
        if let Some(copy_name) = self.copies.get(&(m, c)) {
            return Some(*copy_name);
        }

        match self.constants.defs.get_key_value(&(m, c)) {
            Some((_, ConstantEntry::Defined { loc, signature })) => {
                let defined_loc = *loc;
                let signature = (**signature).clone();
                Some(self.synthesize_constant_copy(m, c, signature, defined_loc, use_loc))
            }
            Some(((_, defined), ConstantEntry::PrecompiledFailed)) => {
                let defined_loc = defined.0.loc;
                self.add_diag(unfoldable_constant_use_error(&m, &c, use_loc, defined_loc));
                None
            }
            Some((_, ConstantEntry::Failed)) => {
                // an error was already reported at the definition in this compilation
                ice_assert!(
                    self.reporter(),
                    self.has_errors(),
                    use_loc,
                    "cross-module constant use of a failed constant"
                );
                None
            }
            None => {
                // a missing entry means typing already errored on this use (due to visibility
                // or similar)
                ice_assert!(
                    self.reporter(),
                    self.has_errors(),
                    use_loc,
                    "cross-module constant use of an unknown constant"
                );
                None
            }
        }
    }

    /// Synthesizes a module-local copy of `m::c` from its already-folded value, named
    /// `const#{address}#{module}#{const}` (e.g. `const#0x42#b#C`). The address component keeps
    /// copies distinct when same-named modules at different addresses are reachable through
    /// non-blocking visibility errors.
    fn synthesize_constant_copy(
        &mut self,
        m: ModuleIdent,
        c: ConstantName,
        signature: H::BaseType,
        defined_loc: Loc,
        use_loc: Loc,
    ) -> ConstantName {
        let address = match &m.value.address {
            Address::Numerical { value, .. } => {
                format!("0x{}", value.value.into_inner().short_str_lossless())
            }
            Address::NamedUnassigned(name) => name.to_string(),
        };

        let symbol: Symbol = format!(
            "const{NEW_NAME_DELIM}{address}{NEW_NAME_DELIM}{}{NEW_NAME_DELIM}{}",
            m.value.module, c
        )
        .into();

        // the copy carries the original definition's loc so errors point at that definition
        let copy_name = ConstantName(sp(defined_loc, symbol));
        let Some(value) = self.constants.values.get(&(m, c)) else {
            self.add_diag(ice!((use_loc, "defined constant with no folded value")));
            return copy_name;
        };
        let cdef = G::Constant {
            warning_filter: empty_filter_scope(),
            index: {
                let index = self.next_index;
                self.next_index += 1;
                index
            },
            attributes: UniqueMap::new(),
            loc: defined_loc,
            signature,
            value: Some(move_value_from_value(value.clone())),
        };

        let prev = self.copies.insert((m, c), copy_name);
        ice_assert!(
            self.reporter(),
            prev.is_none(),
            use_loc,
            "duplicate cross-module constant copy"
        );
        self.copy_defs.push((copy_name, cdef));
        copy_name
    }
}

//**************************************************************************************************
// Entry Point
//**************************************************************************************************

/// Rewrites the module's cross-module constant uses into uses of module-local copies, adding
/// those copies to the module's constants.
pub(crate) fn module(
    context: &CfgirContext,
    constants: &Constants,
    module: ModuleIdent,
    module_constants: &mut UniqueMap<ConstantName, G::Constant>,
    functions: &mut UniqueMap<FunctionName, G::Function>,
) {
    let mut ctxt = Context::new(context, constants, module, module_constants);

    for (_, _, fdef) in functions.iter_mut() {
        let G::FunctionBody_::Defined { blocks, .. } = &mut fdef.body.value else {
            continue;
        };

        for (_, block) in blocks.iter_mut() {
            for cmd in block.iter_mut() {
                command(&mut ctxt, cmd);
            }
        }
    }

    for (name, cdef) in ctxt.into_copies() {
        module_constants
            .add(name, cdef)
            .expect("ICE synthesized constant name collision");
    }
}

fn command(ctxt: &mut Context, sp!(_, cmd_): &mut H::Command) {
    use H::Command_ as C;

    match cmd_ {
        C::IgnoreAndPop { exp: e, .. }
        | C::Return { exp: e, .. }
        | C::Abort(_, e)
        | C::Assign(_, _, e)
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. } => exp(ctxt, e),
        C::Mutate(lhs, rhs) => {
            exp(ctxt, lhs);
            exp(ctxt, rhs)
        }
        C::Break(_) | C::Continue(_) | C::Jump { .. } => (),
    }
}

#[growing_stack]
fn exp(ctxt: &mut Context, e: &mut H::Exp) {
    use H::UnannotatedExp_ as E;

    let eloc = e.exp.loc;

    match &mut e.exp.value {
        e_ @ E::Constant(_, _) => {
            let E::Constant(m, c) = e_ else {
                unreachable!()
            };
            let (m, c) = (*m, *c);
            if m == ctxt.module {
                return;
            }
            if let Some(copy_name) = ctxt.constant_copy(m, c, eloc) {
                *e_ = E::Constant(ctxt.module, copy_name);
            } else {
                // an error was reported at the constant's definition, at this use, or by typing
                ice_assert!(
                    ctxt.reporter(),
                    ctxt.has_errors(),
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
                exp(ctxt, arg);
            }
        }
        E::Freeze(base)
        | E::Dereference(base)
        | E::UnaryExp(_, base)
        | E::Cast(base, _)
        | E::Borrow(_, base, _, _) => exp(ctxt, base),
        E::BinopExp(lhs, _, rhs) => {
            exp(ctxt, lhs);
            exp(ctxt, rhs)
        }
        E::Pack(_, _, fields) | E::PackVariant(_, _, _, fields) => {
            for (_, _, fe) in fields {
                exp(ctxt, fe);
            }
        }
        E::Vector(_, _, _, args) | E::Multiple(args) => {
            for arg in args {
                exp(ctxt, arg);
            }
        }
    }
}
