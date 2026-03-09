// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{LINT_WARNING_PREFIX, LinterDiagnosticCategory, LinterDiagnosticCode};
use crate::{
    diag,
    diagnostics::codes::{DiagnosticInfo, Severity, custom},
    expansion::ast::ModuleIdent,
    parser::ast::FunctionName,
    sui_mode::{
        CLOCK_MODULE_NAME, CLOCK_TYPE_NAME, RANDOMNESS_MODULE_NAME, RANDOMNESS_STATE_TYPE_NAME,
        SUI_ADDR_NAME, SUI_ADDR_VALUE,
        typing::{TxContextKind, is_mut_clock, is_mut_random, tx_context_kind},
    },
    typing::{ast as T, visitor::simple_visitor},
};
use move_ir_types::location::Loc;

const UNCALLABLE_FUNCTION_SIGNATURE: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::UncallableFunction as u8,
    "it will not be possible to call this function",
);

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
                        UNCALLABLE_FUNCTION_SIGNATURE,
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
                        UNCALLABLE_FUNCTION_SIGNATURE,
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
                    let diag = diag!(UNCALLABLE_FUNCTION_SIGNATURE, (param_ty.loc, msg));
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
                let mut diag = diag!(UNCALLABLE_FUNCTION_SIGNATURE, (param_ty.loc, msg),);
                diag.add_note(OBJECT_NOTE);
                self.add_diag(diag);
            }
            if is_mut_random(param_ty) {
                let msg = format!(
                    "Invalid parameter type. '{2}' must be taken immutably, e.g. '&{}::{}::{2}'",
                    SUI_ADDR_NAME, RANDOMNESS_MODULE_NAME, RANDOMNESS_STATE_TYPE_NAME
                );
                let mut diag = diag!(UNCALLABLE_FUNCTION_SIGNATURE, (param_ty.loc, msg));
                diag.add_note(OBJECT_NOTE);
                self.add_diag(diag);
            }
        }
        false
    }
);
