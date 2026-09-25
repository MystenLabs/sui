// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Enforces a signature-level rule on system packages at publish time:
//!
//! > If a function has an `&mut TxContext` parameter and any `&mut _` in its
//! > return list, its parameter list must also contain at least one `&mut U`
//! > for some `U != TxContext`.
//!
//! Gated on `VerifierConfig::framework_tx_context_mut_restrictions`, populated from
//! `ProtocolConfig::framework_tx_context_mut_restrictions()`. Activates at protocol
//! version 131.
//!
//! The rule applies only to system packages: it is a checksum on our own
//! implementations, ensuring no framework function can hand back a mutable
//! reference rooted in the auto-injected `TxContext`. It cannot be enforced
//! generally because user packages can always express the same shape through
//! generic instantiation; for user code, PTB argument arity and auto-injection
//! checks are the actual safety mechanism. User-published modules are exempt
//! (their addresses are freshly generated, never system addresses).
//!
//! Within a system package, functions that do not take `&mut TxContext` are
//! out of scope: they cannot use `TxContext` as a mutable root because they
//! never hold one. The rule covers natives too (framework natives are
//! declared as `native fun` in Move source and appear here as function defs
//! with `None` code). No safelist: any function violating the rule must be
//! reworked, not grandfathered.

use move_binary_format::{
    CompiledModule,
    file_format::{FunctionDefinition, SignatureToken},
};
use move_vm_config::verifier::VerifierConfig;
use sui_types::{
    base_types::{TxContext, TxContextKind},
    error::ExecutionError,
    is_system_package,
};

use crate::verification_failure;

pub fn verify_module(
    module: &CompiledModule,
    verifier_config: &VerifierConfig,
) -> Result<(), ExecutionError> {
    if !verifier_config.framework_tx_context_mut_restrictions {
        return Ok(());
    }
    if !is_system_package(*module.self_id().address()) {
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
            "Function takes `&mut TxContext` and returns a mutable reference, \
             but has no non-`TxContext` `&mut U` parameter. `TxContext` cannot \
             serve as the mutable root for a returned reference; add a mutable \
             reference parameter of another type or return by value.",
        );
    }
    Ok(())
}
