// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Find and write to a file all public or entry calls to a specific function.
use crate::{
    model::{
        global_env::GlobalEnv,
        move_model::{Bytecode, FunctionIndex, Type},
    },
    write_to, CallInfo,
};
use move_binary_format::file_format::Visibility;
use std::{
    collections::BTreeSet,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};
use tracing::error;

pub(crate) fn run(env: &GlobalEnv, output: &Path, call_info: &[CallInfo]) {
    let file = File::create(output.join("function_callers.txt")).unwrap_or_else(|_| {
        panic!(
            "Unable to create file function_callers.txt in {}",
            output.display()
        )
    });
    let buffer = &mut BufWriter::new(file);

    call_info
        .iter()
        .map(|call_info| {
            let func = *env
                .function_map
                .get(&call_info.function)
                .unwrap_or_else(|| panic!("Function {} not found", call_info.function));
            let inst = load_types(env, &call_info.instantiation);
            (func, inst)
        })
        .for_each(|(func, _inst)| {
            // TODO: generic instantiation
            write_to!(buffer, "{}", format_function(env, func));
            let mut visited = BTreeSet::new();
            visited.insert(func);
            let mut to_visit: BTreeSet<_> = env.callees[&func].clone();
            while !to_visit.is_empty() {
                let callee_idx = to_visit.pop_last().unwrap();
                if !visited.insert(callee_idx) {
                    continue;
                }
                let callee = &env.functions[callee_idx];
                if callee.is_entry || callee.visibility == Visibility::Public {
                    write_to!(buffer, "\t{}", format_function(env, callee_idx));
                }
                to_visit.extend(env.callees[&callee_idx].iter().cloned());
            }
        });

    buffer.flush().unwrap();
}

fn load_types(global_env: &GlobalEnv, inst: &[String]) -> Vec<Type> {
    inst.iter()
        .map(|type_name| match type_name.as_str() {
            "bool" => Type::Bool,
            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "u128" => Type::U128,
            "u256" => Type::U256,
            "address" => Type::Address,
            _ => {
                let struct_idx = global_env
                    .struct_map
                    .get(type_name)
                    .unwrap_or_else(|| panic!("Struct {} not found", type_name));
                Type::Struct(*struct_idx)
            }
        })
        .collect()
}

#[allow(unused)]
// TODO: add support for generic calls
fn check_generic_call(
    env: &GlobalEnv,
    func_idx: FunctionIndex,
    caller_idx: FunctionIndex,
    instantiation: &[Type],
) {
    let func = &env.functions[caller_idx];
    if let Some(code) = func.code.as_ref() {
        code.code.iter().for_each(|instr| {
            if let Bytecode::CallGeneric(idx, inst) = instr {
                if *idx == func_idx && instantiation == inst {
                    // TODO: write to file
                }
            }
        })
    }
}

fn format_function(env: &GlobalEnv, idx: FunctionIndex) -> String {
    let func = &env.functions[idx];
    let module = &env.modules[func.module];
    let package = &env.packages[module.package];
    format!(
        "{}::{}::{}",
        package.id,
        env.module_name(module),
        env.function_name(func),
    )
}
