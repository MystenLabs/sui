// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module implements a checker for verifying that a script or entry function a valid
//! signature, which entails
//! - (DEPRECATED) All signer arguments are occur before non-signer arguments
//! - (DEPRECATED) All types non-signer arguments have a type that is valid for constants
//! - (DEPRECATED) Has an empty return type
//! - All return types are not references
//! - Satisfies the additional checks provided as an argument via `check_signature`
//! `check_signature` should be used by adapters to quickly and easily verify custom signature
//! rules for entrypoints

use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{
        CompiledModule, CompiledScript, FunctionDefinitionIndex, SignatureIndex, SignatureToken,
        TableIndex,
    },
    file_format_common::{VERSION_1, VERSION_5},
    IndexKind,
};
use move_core_types::{identifier::IdentStr, vm_status::StatusCode};

pub type FnCheckScriptSignature = fn(
    &BinaryIndexedView,
    /* is_entry */ bool,
    SignatureIndex,
    Option<SignatureIndex>,
) -> PartialVMResult<()>;

/// This function checks the extra requirements on the signature of the main function of a script.
pub fn verify_script(
    script: &CompiledScript,
    check_signature: FnCheckScriptSignature,
) -> VMResult<()> {
    if script.version >= VERSION_5 {
        return Ok(());
    }

    let resolver = &BinaryIndexedView::Script(script);
    let parameters = script.parameters;
    let return_ = None;
    verify_main_signature_impl(resolver, true, parameters, return_, check_signature)
        .map_err(|e| e.finish(Location::Script))
}

pub fn verify_module(
    module: &CompiledModule,
    check_signature: FnCheckScriptSignature,
) -> VMResult<()> {
    // important for not breaking old modules
    if module.version < VERSION_5 {
        return Ok(());
    }

    for (idx, _fdef) in module
        .function_defs()
        .iter()
        .enumerate()
        .filter(|(_idx, fdef)| fdef.is_entry)
    {
        verify_module_function_signature(
            module,
            FunctionDefinitionIndex(idx as TableIndex),
            check_signature,
        )?
    }
    Ok(())
}

/// This function checks the extra requirements on the signature of the script visible function
/// when it serves as an entry point for script execution
pub fn verify_module_function_signature_by_name(
    module: &CompiledModule,
    name: &IdentStr,
    check_signature: FnCheckScriptSignature,
) -> VMResult<()> {
    let fdef_opt = module.function_defs().iter().enumerate().find(|(_, fdef)| {
        module.identifier_at(module.function_handle_at(fdef.function).name) == name
    });
    let (idx, _fdef) = fdef_opt.ok_or_else(|| {
        PartialVMError::new(StatusCode::VERIFICATION_ERROR)
            .with_message("function not found in verify_module_script_function".to_string())
            .finish(Location::Module(module.self_id()))
    })?;
    verify_module_function_signature(
        module,
        FunctionDefinitionIndex(idx as TableIndex),
        check_signature,
    )
}

/// This function checks the extra requirements on the signature of the script visible function
/// when it serves as an entry point for script execution
fn verify_module_function_signature(
    module: &CompiledModule,
    idx: FunctionDefinitionIndex,
    check_signature: FnCheckScriptSignature,
) -> VMResult<()> {
    let fdef = module.function_def_at(idx);

    let resolver = &BinaryIndexedView::Module(module);
    let fhandle = module.function_handle_at(fdef.function);
    let parameters = fhandle.parameters;
    let return_ = fhandle.return_;
    verify_main_signature_impl(
        resolver,
        fdef.is_entry,
        parameters,
        Some(return_),
        check_signature,
    )
    .map_err(|e| {
        e.at_index(IndexKind::FunctionDefinition, idx.0)
            .finish(Location::Module(module.self_id()))
    })
}

fn verify_main_signature_impl(
    resolver: &BinaryIndexedView,
    is_entry: bool,
    parameters_idx: SignatureIndex,
    return_idx: Option<SignatureIndex>,
    check_signature: FnCheckScriptSignature,
) -> PartialVMResult<()> {
    let deprecated_logic = resolver.version() < VERSION_5 && is_entry;

    if deprecated_logic {
        legacy_script_signature_checks(resolver, is_entry, parameters_idx, return_idx)?;
    }
    check_signature(resolver, is_entry, parameters_idx, return_idx)
}

pub fn no_additional_script_signature_checks(
    _resolver: &BinaryIndexedView,
    _is_entry: bool,
    _parameters: SignatureIndex,
    _return_type: Option<SignatureIndex>,
) -> PartialVMResult<()> {
    Ok(())
}

pub fn legacy_script_signature_checks(
    resolver: &BinaryIndexedView,
    _is_entry: bool,
    parameters_idx: SignatureIndex,
    return_idx: Option<SignatureIndex>,
) -> PartialVMResult<()> {
    use SignatureToken as S;
    let empty_vec = &vec![];
    let parameters = &resolver.signature_at(parameters_idx).0;
    let return_types = return_idx
        .map(|idx| &resolver.signature_at(idx).0)
        .unwrap_or(empty_vec);
    // Check that all `signer` arguments occur before non-`signer` arguments
    // signer is a type that can only be populated by the Move VM. And its value is filled
    // based on the sender of the transaction
    let all_args_have_valid_type = if resolver.version() <= VERSION_1 {
        parameters
            .iter()
            .skip_while(|typ| matches!(typ, S::Reference(inner) if matches!(&**inner, S::Signer)))
            .all(|typ| typ.is_valid_for_constant())
    } else {
        parameters
            .iter()
            .skip_while(|typ| matches!(typ, S::Signer))
            .all(|typ| typ.is_valid_for_constant())
    };
    let has_valid_return_type = return_types.is_empty();
    if !all_args_have_valid_type || !has_valid_return_type {
        Err(PartialVMError::new(
            StatusCode::INVALID_MAIN_FUNCTION_SIGNATURE,
        ))
    } else {
        Ok(())
    }
}
