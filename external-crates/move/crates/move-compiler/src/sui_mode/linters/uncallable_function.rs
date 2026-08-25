// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::SuiLintCode;
use crate::{
    diag,
    expansion::ast::{ModuleIdent, Visibility},
    naming::ast::{Type, TypeInner, TypeName_},
    parser::ast::FunctionName,
    sui_mode::{
        CLOCK_MODULE_NAME, CLOCK_TYPE_NAME, RANDOMNESS_MODULE_NAME, RANDOMNESS_STATE_TYPE_NAME,
        SUI_ADDR_NAME, SUI_ADDR_VALUE,
        typing::{TxContextKind, is_mut_clock, is_mut_random, tx_context_kind},
    },
    typing::{ast as T, visitor::simple_visitor},
};
use move_ir_types::location::Loc;

simple_visitor!(
    UncallableFunction,
    fn visit_module_custom(&mut self, ident: ModuleIdent, mdef: &T::ModuleDefinition) -> bool {
        // skip if in  `sui` or test
        ident.value.address.is(&SUI_ADDR_VALUE) || mdef.attributes.is_test_or_test_only()
    },
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        if fdef.attributes.is_test_or_test_only() {
            return false;
        }
        // check tx context usage
        const DUPLICATE_TX_CTX_NOTE: &str = "Due to restrictions in PTB execution if there is a \
            mutable reference to a TxContext, it must be unique. This means that there cannot be \
            another usage of the transaction context (either '&TxContext' or '&mut TxContext') in \
            the function parameters. This function will not be callable on Sui";
        let signature = &fdef.signature;
        // Check for multiple TxContexts
        let mut prev_tx_ctx: Option<(Loc, /* mut */ bool)> = None;
        for (_, _, param_ty) in &signature.parameters {
            let Some(tx_ctx_kind) = tx_context_kind(param_ty) else {
                continue;
            };
            match (tx_ctx_kind, &prev_tx_ctx) {
                (TxContextKind::None, _) => (),
                (TxContextKind::Mutable, None) => prev_tx_ctx = Some((param_ty.loc, true)),
                (TxContextKind::Immutable, None) | (TxContextKind::Immutable, Some((_, false))) => {
                    prev_tx_ctx = Some((param_ty.loc, false))
                }
                (TxContextKind::Mutable, Some((prev_loc, _prev_kind))) => {
                    let mut_msg =
                        "Duplicate 'TxContext' usage. '&mut TxContext' usage must be unique";
                    let mut diag = diag!(
                        SuiLintCode::UncallableFunction.diag_info(),
                        (param_ty.loc, mut_msg),
                        (*prev_loc, "Previous 'TxContext' usage here")
                    );
                    diag.add_note(DUPLICATE_TX_CTX_NOTE);
                    self.add_diag(diag);
                    break;
                }
                (TxContextKind::Immutable, Some((prev_loc, true))) => {
                    let mut_msg =
                        "Previous 'TxContext' usage here. '&mut TxContext' usage must be unique";
                    let mut diag = diag!(
                        SuiLintCode::UncallableFunction.diag_info(),
                        (param_ty.loc, "Duplicate TxContext usage"),
                        (*prev_loc, mut_msg)
                    );
                    diag.add_note(DUPLICATE_TX_CTX_NOTE);
                    self.add_diag(diag);
                    break;
                }
                (TxContextKind::Owned, _) => {
                    let msg = "Invalid TxContext usage. 'TxContext' must be taken by reference, \
                    e.g. '&TxContext' or '&mut TxContext'";
                    let diag = diag!(
                        SuiLintCode::UncallableFunction.diag_info(),
                        (param_ty.loc, msg)
                    );
                    self.add_diag(diag);
                    break;
                }
            }
        }

        // extra warnings for entry functions
        const OBJECT_NOTE: &str = "This object has extra restrictions checked when submitting \
        transactions to Sui. As such, this function will not be callable.";
        for (_, _, param_ty) in &signature.parameters {
            if is_mut_clock(param_ty) {
                let msg = format!(
                    "Invalid parameter type. '{2}' must be taken immutably, e.g. '&{}::{}::{2}'",
                    SUI_ADDR_NAME, CLOCK_MODULE_NAME, CLOCK_TYPE_NAME
                );
                let mut diag = diag!(
                    SuiLintCode::UncallableFunction.diag_info(),
                    (param_ty.loc, msg),
                );
                diag.add_note(OBJECT_NOTE);
                self.add_diag(diag);
            }
            if is_mut_random(param_ty) {
                let msg = format!(
                    "Invalid parameter type. '{2}' must be taken immutably, e.g. '&{}::{}::{2}'",
                    SUI_ADDR_NAME, RANDOMNESS_MODULE_NAME, RANDOMNESS_STATE_TYPE_NAME
                );
                let mut diag = diag!(
                    SuiLintCode::UncallableFunction.diag_info(),
                    (param_ty.loc, msg)
                );
                diag.add_note(OBJECT_NOTE);
                self.add_diag(diag);
            }
        }

        // `TxContext` can never appear in return position for a function callable from a
        // transaction. Only public and entry functions can be called that way; private helpers
        // returning references derived from a `TxContext` parameter are fine
        if matches!(&fdef.visibility, Visibility::Public(_)) || fdef.entry.is_some() {
            const RETURN_NOTE: &str = "Due to restrictions in PTB execution, 'TxContext' may \
                never appear in the return type of a function callable from a transaction, by \
                value or by reference. This function will not be callable from PTBs on Sui";
            for ret_ty in return_position_types(&signature.return_type) {
                match tx_context_kind(ret_ty) {
                    None | Some(TxContextKind::None) => (),
                    Some(
                        TxContextKind::Owned | TxContextKind::Mutable | TxContextKind::Immutable,
                    ) => {
                        let msg = "Invalid return type. 'TxContext' cannot be returned";
                        let mut diag = diag!(
                            SuiLintCode::UncallableFunction.diag_info(),
                            (ret_ty.loc, msg)
                        );
                        diag.add_note(RETURN_NOTE);
                        self.add_diag(diag);
                    }
                }
            }
        }
        false
    }
);

/// The individual types in return position: the elements for a tuple return, otherwise the type
/// itself
fn return_position_types(ret_ty: &Type) -> Vec<&Type> {
    match ret_ty.value.inner() {
        TypeInner::Apply(_, sp!(_, TypeName_::Multiple(_)), tys) => tys.iter().collect(),
        _ => vec![ret_ty],
    }
}
