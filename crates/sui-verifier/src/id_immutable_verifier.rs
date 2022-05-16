// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The ID field of structs with key ability is immutable since it represents a
//! Sui object ID. The only way to mutate a field is to write into
//! a mutable reference borrowed through MutBorrowField/MutBorrowFieldGeneric
//! bytecode. This verifier checks that such bytecode is never operated on
//! an ID field.

use crate::verification_failure;
use move_binary_format::{
    binary_views::BinaryIndexedView,
    file_format::{Bytecode, CompiledModule, FieldHandleIndex},
};
use sui_types::error::SuiResult;

pub fn verify_module(module: &CompiledModule) -> SuiResult {
    verify_id_immutable(module)
}

fn verify_id_immutable(module: &CompiledModule) -> SuiResult {
    let view = BinaryIndexedView::Module(module);
    for func_def in &module.function_defs {
        if func_def.code.is_none() {
            continue;
        }
        let code = &func_def.code.as_ref().unwrap().code;
        let check = |field_idx: FieldHandleIndex| {
            let field = view.field_handle_at(field_idx).unwrap();
            let struct_idx = view.struct_def_at(field.owner).unwrap().struct_handle;
            // The struct_with_key_verifier already checked that the first field of a key struct
            // must be the ID field.
            if view.struct_handle_at(struct_idx).abilities.has_key() && field.field == 0 {
                return Err(verification_failure(format!(
                    "In function {}: ID field of struct {} cannot be mut borrowed because ID is immutable.",
                    view.identifier_at(view.function_handle_at(func_def.function).name),
                    view.identifier_at(view.struct_handle_at(struct_idx).name))));
            }
            Ok(())
        };
        for bytecode in code {
            match bytecode {
                Bytecode::MutBorrowField(field_idx) => {
                    check(*field_idx)?;
                }
                Bytecode::MutBorrowFieldGeneric(field_idx) => {
                    let field = view.field_instantiation_at(*field_idx).unwrap();
                    check(field.handle)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}
