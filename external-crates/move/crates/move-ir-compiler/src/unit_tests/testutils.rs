// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_binary_format::{
    errors::{Location, VMError},
    file_format::CompiledModule,
};
use move_bytecode_verifier::verify_module_unmetered;
use move_ir_to_bytecode::{compiler::compile_module, parser::parse_module};

fn compile_module_string_impl(
    code: &str,
    deps: Vec<CompiledModule>,
) -> Result<(CompiledModule, Option<VMError>)> {
    let module = parse_module(code).unwrap();
    let compiled_module = compile_module(module, &deps)?.0;

    let mut serialized_module = Vec::<u8>::new();
    compiled_module.serialize_with_version(compiled_module.version, &mut serialized_module)?;
    let deserialized_module = CompiledModule::deserialize_with_defaults(&serialized_module)
        .map_err(|e| e.finish(Location::Undefined))?;
    assert_eq!(compiled_module, deserialized_module);

    // Always return a CompiledModule because some callers explicitly care about unverified
    // modules.
    Ok(match verify_module_unmetered(&compiled_module) {
        Ok(_) => (compiled_module, None),
        Err(error) => (compiled_module, Some(error)),
    })
}

fn compile_module_string_and_assert_no_error(
    code: &str,
    deps: Vec<CompiledModule>,
) -> Result<CompiledModule> {
    let (verified_module, verification_error) = compile_module_string_impl(code, deps)?;
    assert!(verification_error.is_none());
    Ok(verified_module)
}

pub fn compile_module_string(code: &str) -> Result<CompiledModule> {
    compile_module_string_and_assert_no_error(code, vec![])
}
