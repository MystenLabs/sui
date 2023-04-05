// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_binary_format::{
    errors::{Location, VMError},
    file_format::{CompiledModule, CompiledScript},
};
use move_bytecode_verifier::{verify_module, verify_script};
use move_ir_to_bytecode::{
    compiler::{compile_module, compile_script},
    parser::{parse_module, parse_script},
};

fn compile_script_string_impl(
    code: &str,
    deps: Vec<CompiledModule>,
) -> Result<(CompiledScript, Option<VMError>)> {
    let parsed_script = parse_script(code).unwrap();
    let script = compile_script(parsed_script, &deps)?.0;

    let mut serialized_script = Vec::<u8>::new();
    script.serialize(&mut serialized_script)?;
    let deserialized_script = CompiledScript::deserialize(&serialized_script)
        .map_err(|e| e.finish(Location::Undefined).into_vm_status())?;
    assert_eq!(script, deserialized_script);

    Ok(match verify_script(&script) {
        Ok(_) => (script, None),
        Err(error) => (script, Some(error)),
    })
}

fn compile_script_string_and_assert_no_error(
    code: &str,
    deps: Vec<CompiledModule>,
) -> Result<CompiledScript> {
    let (verified_script, verification_error) = compile_script_string_impl(code, deps)?;
    assert!(verification_error.is_none());
    Ok(verified_script)
}

pub fn compile_script_string(code: &str) -> Result<CompiledScript> {
    compile_script_string_and_assert_no_error(code, vec![])
}

fn compile_module_string_impl(
    code: &str,
    deps: Vec<CompiledModule>,
) -> Result<(CompiledModule, Option<VMError>)> {
    let module = parse_module(code).unwrap();
    let compiled_module = compile_module(module, &deps)?.0;

    let mut serialized_module = Vec::<u8>::new();
    compiled_module.serialize(&mut serialized_module)?;
    let deserialized_module = CompiledModule::deserialize(&serialized_module)
        .map_err(|e| e.finish(Location::Undefined).into_vm_status())?;
    assert_eq!(compiled_module, deserialized_module);

    // Always return a CompiledModule because some callers explicitly care about unverified
    // modules.
    Ok(match verify_module(&compiled_module) {
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
