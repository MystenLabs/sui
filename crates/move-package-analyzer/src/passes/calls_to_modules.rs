// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    model::{
        global_env::GlobalEnv,
        move_model::{Bytecode, FunctionIndex},
        walkers::walk_bytecodes,
    },
    write_to,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};
use tracing::error;

/// Find and write to a file all functions calling a specific module.
pub(crate) fn run(env: &GlobalEnv, output: &Path, modules: &[String]) {
    let file = File::create(output.join("calls_to_modules.txt")).unwrap_or_else(|_| {
        panic!(
            "Unable to create file calls_to_modules.txt in {}",
            output.display()
        )
    });
    let buffer = &mut BufWriter::new(file);

    modules.iter().for_each(|module_name| {
        write_to!(buffer, "Module {}:", module_name);
        let module_idx = env
            .module_map
            .get(module_name)
            .unwrap_or_else(|| panic!("Module {} not found", module_name));
        walk_bytecodes(env, |env, func, bytecode| match bytecode {
            Bytecode::Call(func_idx) | Bytecode::CallGeneric(func_idx, _) => {
                let call = &env.functions[*func_idx];
                if call.module == *module_idx {
                    write_to!(
                        buffer,
                        "\t{} => {}",
                        format_function(env, func.self_idx),
                        env.function_name(call),
                    );
                }
            }
            _ => (),
        });
    });

    buffer.flush().unwrap();
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
