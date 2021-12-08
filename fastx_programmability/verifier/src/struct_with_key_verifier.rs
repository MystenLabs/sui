// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use fastpay_core::{
    error::{FastPayError, FastPayResult},
    fp_ensure,
};
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{CompiledModule, SignatureToken},
};

pub fn verify_module(module: &CompiledModule) -> FastPayResult {
    verify_key_structs(module)
}

fn verify_key_structs(module: &CompiledModule) -> FastPayResult {
    let view = BinaryIndexedView::Module(module);
    let struct_defs = &module.struct_defs;
    for def in struct_defs {
        let handle = module.struct_handle_at(def.struct_handle);
        if !handle.abilities.has_key() {
            continue;
        }
        let name = view.identifier_at(handle.name);
        fp_ensure!(
            !handle.abilities.has_drop(),
            FastPayError::ModuleVerificationFailure {
                error: format!("Struct {} cannot have both key and drop abilities", name)
            }
        );

        let id_error = FastPayError::ModuleVerificationFailure {
            error: format!(
                "First field of struct {} must be 'id' with type 'ID' since it has 'key' ability",
                name
            ),
        };
        let first_field = match def.field(0) {
            Some(field) => field,
            None => return Err(id_error),
        };
        fp_ensure!(
            view.identifier_at(first_field.name).as_str() == "id",
            id_error
        );
        let id_field_type = match first_field.signature.0 {
            SignatureToken::Struct(struct_type) => struct_type,
            _ => return Err(id_error),
        };
        fp_ensure!(
            view.identifier_at(module.struct_handle_at(id_field_type).name)
                .as_str()
                == "ID",
            id_error
        );
    }
    Ok(())
}
