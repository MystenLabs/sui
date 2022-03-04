// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This verifier only runs when we are trying to publish a module onchain.
//! Hence it contains publish-specific verification rules, such as no calls to Debug module.

use crate::verification_failure;
use move_binary_format::{
    binary_views::BinaryIndexedView,
    file_format::{Bytecode, CompiledModule, FunctionHandleIndex},
};
use sui_types::{error::SuiResult, SUI_FRAMEWORK_ADDRESS};

pub fn verify_module(module: &CompiledModule) -> SuiResult {
    verify_no_debug_calls(module)
}

/// Checks that whether any debug code is used when publishing a module onchain.
fn verify_no_debug_calls(module: &CompiledModule) -> SuiResult {
    let view = BinaryIndexedView::Module(module);
    for func_def in &module.function_defs {
        if func_def.code.is_none() {
            continue;
        }
        let code = &func_def.code.as_ref().unwrap().code;
        for bytecode in code {
            match bytecode {
                Bytecode::Call(idx) => check_call(&view, idx)?,
                Bytecode::CallGeneric(idx) => {
                    check_call(&view, &view.function_instantiation_at(*idx).handle)?
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn check_call(view: &BinaryIndexedView, func_idx: &FunctionHandleIndex) -> SuiResult {
    let handle = view.function_handle_at(*func_idx);
    let module_idx = handle.module;
    let module_handle = view.module_handle_at(module_idx);
    if view.address_identifier_at(module_handle.address) == &SUI_FRAMEWORK_ADDRESS
        && view.identifier_at(module_handle.name).as_str() == "Debug"
    {
        Err(verification_failure(
            format!("Calls to Debug module not allowed when publishing code onchain. Found in function '{:?}'", view.identifier_at(handle.name))
        ))
    } else {
        Ok(())
    }
}
