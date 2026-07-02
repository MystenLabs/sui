// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Enforces a signature-level rule at Move module publish time:
//!
//! > If a function has an `&mut TxContext` parameter and returns `&mut T` for
//! > some `T != TxContext`, its parameter list must also contain at least
//! > one `&mut U` where `U != TxContext`.
//!
//! Gated on `VerifierConfig::check_tx_context_restrictions`, which is
//! populated from `ProtocolConfig::allow_references_in_ptbs`. The pass
//! activates in the same protocol version as Section 1.
//!
//! Functions that do not take `&mut TxContext` are out of scope: they cannot
//! use `TxContext` as a mutable root because they never hold one. The rule
//! applies uniformly to every in-scope function definition in every module,
//! natives included (framework natives are declared as `native fun` in Move
//! source and appear here as function defs with `None` code). No safelist:
//! any function violating the rule must be reworked, not grandfathered.

use move_binary_format::{
    CompiledModule,
    file_format::{FunctionDefinition, SignatureToken},
};
use move_vm_config::verifier::VerifierConfig;
use sui_types::{
    base_types::{TxContext, TxContextKind},
    error::ExecutionError,
};

use crate::verification_failure;

pub fn verify_module(
    module: &CompiledModule,
    verifier_config: &VerifierConfig,
) -> Result<(), ExecutionError> {
    if !verifier_config.check_tx_context_restrictions {
        return Ok(());
    }
    for func_def in &module.function_defs {
        verify_function(module, func_def).map_err(|error| {
            let name = module.identifier_at(module.function_handle_at(func_def.function).name);
            verification_failure(format!("{}::{}. {}", module.self_id(), name, error))
        })?;
    }
    Ok(())
}

fn verify_function(module: &CompiledModule, fdef: &FunctionDefinition) -> Result<(), &'static str> {
    let fhandle = module.function_handle_at(fdef.function);
    let returns = &module.signature_at(fhandle.return_).0;
    let safe_returns = returns
        .iter()
        .all(|t| !matches!(t, SignatureToken::MutableReference(_)));
    if safe_returns {
        return Ok(());
    }
    let params = &module.signature_at(fhandle.parameters).0;
    let (tx_context_muts, other_muts): (Vec<_>, Vec<_>) = params
        .iter()
        .filter(|t| matches!(t, SignatureToken::MutableReference(_)))
        .partition(|t| TxContext::kind(module, t) == TxContextKind::Mutable);
    if !tx_context_muts.is_empty() && other_muts.is_empty() {
        return Err(
            "Function takes `&mut TxContext` and returns `&mut T` for some \
             `T != TxContext`, but has no non-`TxContext` `&mut U` parameter. \
             `TxContext` cannot serve as the mutable root for a returned reference \
             to a different type; add a mutable reference parameter to that type \
             or return by value.",
        );
    }
    Ok(())
}
