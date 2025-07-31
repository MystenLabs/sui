// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::CompiledModule;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;

pub(crate) fn disassemble(module: &CompiledModule) -> anyhow::Result<String> {
    let d = Disassembler::from_module(module, Spanned::unsafe_no_loc(()).loc)?;
    let disassemble_string = d.disassemble()?;
    // let (disassemble_string, _) = d.disassemble_with_source_map()?;

    // println!("{}", disassemble_string);
    Ok(disassemble_string)
}

pub(crate) fn comma_separated<T: std::fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(|item| format!("{}", item))
        .collect::<Vec<_>>()
        .join(", ")
}
