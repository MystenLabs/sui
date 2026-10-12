// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rewrites a module's cross-module constant uses into uses of module-local copies, synthesized
//! on demand from the values `folding` computed.

use crate::{
    cfgir::{
        ast as G,
        constants::{ConstantEntry, ConstantId, Constants, unfoldable_constant_use_error},
        translate::{Context as CfgirContext, move_value_from_value},
        visitor::CFGIRMutVisitorContext,
    },
    diagnostics::{
        Diagnostic, DiagnosticReporter,
        filter::{FilterScope, empty_filter_scope},
    },
    expansion::ast::{Address, ModuleIdent},
    hlir::{ast as H, translate::NEW_NAME_DELIM},
    ice, ice_assert,
    parser::ast::{ConstantName, FunctionName},
    shared::unique_map::UniqueMap,
};

use move_ir_types::location::*;
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

    for (name, fdef) in functions.key_cloned_iter_mut() {
        ctxt.visit_function(module, name, fdef);
    }

    for (name, cdef) in ctxt.into_copies() {
        module_constants
            .add(name, cdef)
            .expect("ICE synthesized constant name collision");
    }
}

impl CFGIRMutVisitorContext for Context<'_, '_> {
    fn push_warning_filter_scope(&mut self, _filters: FilterScope) {}

    fn pop_warning_filter_scope(&mut self) {}

    fn visit_exp_custom(&mut self, e: &mut H::Exp) -> bool {
        use H::UnannotatedExp_ as E;

        let eloc = e.exp.loc;
        let E::Constant(m, c) = e.exp.value else {
            return false;
        };
        if m == self.module {
            return true;
        }
        if let Some(copy_name) = self.constant_copy(m, c, eloc) {
            e.exp.value = E::Constant(self.module, copy_name);
        } else {
            // an error was reported at the constant's definition, at this use, or by typing
            ice_assert!(
                self.reporter(),
                self.has_errors(),
                eloc,
                "cross-module constant use failed to resolve to a module-local copy"
            );
            e.exp.value = E::UnresolvedError;
        }
        true
    }
}
