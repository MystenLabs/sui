// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{
        Bytecode, FunctionDefinition, FunctionInstantiation, ModuleHandle, SignatureToken,
    },
    CompiledModule,
};
use sui_types::{
    error::{SuiError, SuiResult},
    SUI_FRAMEWORK_ADDRESS,
};

use crate::format_signature_token;

/// All transfer functions (the functions in `Sui::Transfer`) are "private" in that they are
/// restricted to the module.
/// For example, with `Transfer::transfer<T>(...)`, either:
/// - `T` must be a type declared in the current module or
/// - `T` must have `store`
pub fn verify_module(module: &CompiledModule) -> SuiResult {
    let view = &BinaryIndexedView::Module(module);
    // do not need to check the Sui::Transfer module itself
    if is_transfer_module(view, module.self_handle()) {
        return Ok(());
    }
    for func_def in &module.function_defs {
        verify_function(view, func_def).map_err(|error| SuiError::ModuleVerificationFailure {
            error: format!(
                "{}::{}. {}",
                module.self_id(),
                module.identifier_at(module.function_handle_at(func_def.function).name),
                error
            ),
        })?;
    }
    Ok(())
}

fn verify_function(view: &BinaryIndexedView, fdef: &FunctionDefinition) -> Result<(), String> {
    let code = match &fdef.code {
        None => return Ok(()),
        Some(code) => code,
    };
    let function_type_parameters = &view.function_handle_at(fdef.function).type_parameters;
    for instr in &code.code {
        if let Bytecode::CallGeneric(finst_idx) = instr {
            let FunctionInstantiation {
                handle,
                type_parameters,
            } = view.function_instantiation_at(*finst_idx);

            let fhandle = view.function_handle_at(*handle);
            let mhandle = view.module_handle_at(fhandle.module);

            let type_arguments = &view.signature_at(*type_parameters).0;
            if !is_transfer_module(view, mhandle) {
                continue;
            }
            let fident = view.identifier_at(fhandle.name);
            match fident.as_str() {
                // transfer functions
                "transfer"
                | "transfer_to_object_id"
                | "freeze_object"
                | "share_object"
                | "transfer_to_object"
                | "transfer_to_object_unsafe"
                | "transfer_child_to_object"
                | "transfer_child_to_address" => (),
                // these functions operate over ChildRef
                "is_child" | "is_child_unsafe" | "delete_child_object" => {
                    continue;
                }
                // should be unreachable
                // these are private and the module itself is skipped
                "transfer_internal" | "delete_child_object_internal" => {
                    debug_assert!(false);
                    continue;
                }
                // unknown function, so a bug in the implementation here
                s => {
                    debug_assert!(false, "unknown transfer function {}", s);
                    continue;
                }
            };
            for type_arg in type_arguments {
                let has_store = view
                    .abilities(type_arg, function_type_parameters)
                    .map_err(|vm_err| vm_err.to_string())?
                    .has_store();
                let is_defined_in_current_module = match type_arg {
                    SignatureToken::TypeParameter(_) => false,
                    SignatureToken::Struct(idx) | SignatureToken::StructInstantiation(idx, _) => {
                        let shandle = view.struct_handle_at(*idx);
                        view.self_handle_idx() == Some(shandle.module)
                    }
                    // do not have key or cannot instantiate generics
                    // should be caught already by Move's bytecode verifier
                    SignatureToken::Bool
                    | SignatureToken::U8
                    | SignatureToken::U64
                    | SignatureToken::U128
                    | SignatureToken::Address
                    | SignatureToken::Signer
                    | SignatureToken::Vector(_)
                    | SignatureToken::Reference(_)
                    | SignatureToken::MutableReference(_) => {
                        debug_assert!(false);
                        false
                    }
                };
                if !has_store && !is_defined_in_current_module {
                    return Err(format!(
                        "Invalid call to '{}::Transfer::{}'. \
                        Invalid transfer of object of type '{}'. \
                        The transferred object's type must be defined in the current module, \
                        or must have the 'store' type ability",
                        SUI_FRAMEWORK_ADDRESS,
                        fident,
                        format_signature_token(view, type_arg),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn is_transfer_module(view: &BinaryIndexedView, mhandle: &ModuleHandle) -> bool {
    let maddr = view.address_identifier_at(mhandle.address);
    let mident = view.identifier_at(mhandle.name);
    maddr == &SUI_FRAMEWORK_ADDRESS && mident.as_str() == "Transfer"
}
