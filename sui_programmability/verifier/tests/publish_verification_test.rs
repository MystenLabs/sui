// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod common;

pub use common::module_builder::ModuleBuilder;
use move_binary_format::file_format::*;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_verifier::publish_verifier::verify_module;

#[test]
fn function_with_debug_call() {
    let (mut module, _) = ModuleBuilder::default();
    // Add mock Debug module.
    let debug_module = module.add_module(SUI_FRAMEWORK_ADDRESS, "Debug");
    let print_func1 = module.add_function(debug_module, "print1", vec![], vec![]);
    let print_func2 = module.add_generic_function(debug_module, "print2", vec![], vec![], vec![]);
    let func = module.add_function(module.get_self_index(), "foo", vec![], vec![]);
    assert!(verify_module(module.get_module()).is_ok());

    // Bytecode that contains a call to Debug::print.
    let code = vec![Bytecode::Call(print_func1.handle)];
    module.set_bytecode(func.def, code);
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains("Calls to Debug module not allowed when publishing code onchain"));

    let code = vec![Bytecode::CallGeneric(print_func2.handle)];
    module.set_bytecode(func.def, code);
    assert!(verify_module(module.get_module())
        .unwrap_err()
        .to_string()
        .contains("Calls to Debug module not allowed when publishing code onchain"));
}
