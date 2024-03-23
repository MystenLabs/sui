// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Utilities functions for working with `CompiledModules`.
use move_binary_format::file_format::{
    FunctionDefinitionIndex, FunctionHandleIndex, StructDefinitionIndex, StructHandleIndex,
};
use move_binary_format::CompiledModule;
use move_core_types::account_address::AccountAddress;

/// Get the module and struct name from a struct handle index.
pub fn module_struct_name_from_handle(
    module: &CompiledModule,
    struct_handle_idx: StructHandleIndex,
) -> (&str, &str) {
    let struct_handle = &module.struct_handles[struct_handle_idx.0 as usize];
    let module_handle = &module.module_handles[struct_handle.module.0 as usize];
    let module_name = module.identifiers[module_handle.name.0 as usize].as_str();
    let struct_name = module.identifiers[struct_handle.name.0 as usize].as_str();
    (module_name, struct_name)
}

/// Get the module and struct name from a struct definition index.
pub fn module_struct_name_from_def(
    module: &CompiledModule,
    struct_def_idx: StructDefinitionIndex,
) -> (&str, &str) {
    let struct_handle_idx = module.struct_defs[struct_def_idx.0 as usize].struct_handle;
    module_struct_name_from_handle(module, struct_handle_idx)
}

/// Get the module and function name from a function handle index.
pub fn module_function_name_from_handle(
    module: &CompiledModule,
    func_handle_idx: FunctionHandleIndex,
) -> (&str, &str) {
    let func_handle = &module.function_handles[func_handle_idx.0 as usize];
    let module_handle = &module.module_handles[func_handle.module.0 as usize];
    let module_name = module.identifiers[module_handle.name.0 as usize].as_str();
    let func_name = module.identifiers[func_handle.name.0 as usize].as_str();
    (module_name, func_name)
}

/// Get the module and function name from a function definition index.
pub fn module_function_name_from_def(
    module: &CompiledModule,
    func_def_idx: FunctionDefinitionIndex,
) -> (&str, &str) {
    let func_handle_idx = module.function_defs[func_def_idx.0 as usize].function;
    module_function_name_from_handle(module, func_handle_idx)
}

/// Get the package address from a struct handle index.
pub fn get_package_from_struct_handle(
    module: &CompiledModule,
    struct_handle_idx: StructHandleIndex,
) -> AccountAddress {
    let struct_handle = &module.struct_handles[struct_handle_idx.0 as usize];
    let module_handle = &module.module_handles[struct_handle.module.0 as usize];
    module.address_identifiers[module_handle.address.0 as usize]
}

/// Get the package address from a struct definition index.
pub fn get_package_from_struct_def(
    module: &CompiledModule,
    struct_def_idx: StructDefinitionIndex,
) -> AccountAddress {
    let struct_def = &module.struct_defs[struct_def_idx.0 as usize];
    let struct_handle = &module.struct_handles[struct_def.struct_handle.0 as usize];
    let module_handle = &module.module_handles[struct_handle.module.0 as usize];
    module.address_identifiers[module_handle.address.0 as usize]
}

/// Get the package address from a function handle index.
pub fn get_package_from_function_handle(
    module: &CompiledModule,
    func_handle_idx: FunctionHandleIndex,
) -> AccountAddress {
    let function_handle = &module.function_handles[func_handle_idx.0 as usize];
    let module_handle = &module.module_handles[function_handle.module.0 as usize];
    module.address_identifiers[module_handle.address.0 as usize]
}
