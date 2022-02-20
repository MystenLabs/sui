// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::verification_failure;
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{CompiledModule, SignatureToken},
};
use sui_types::{error::SuiResult, fp_ensure, SUI_FRAMEWORK_ADDRESS};

pub fn verify_module(module: &CompiledModule) -> SuiResult {
    verify_key_structs(module)
}

fn verify_key_structs(module: &CompiledModule) -> SuiResult {
    let view = BinaryIndexedView::Module(module);
    let struct_defs = &module.struct_defs;
    for def in struct_defs {
        let handle = module.struct_handle_at(def.struct_handle);
        if !handle.abilities.has_key() {
            continue;
        }
        let name = view.identifier_at(handle.name);
        // Check that a struct with key ability must not have drop ability.
        // A struct with key ability represents a sui object.
        // We want to ensure that sui objects cannot be arbitrarily dropped.
        // For example, *x = new_object shouldn't work for a key object x.
        fp_ensure!(
            !handle.abilities.has_drop(),
            verification_failure(format!(
                "Struct {} cannot have both key and drop abilities",
                name
            ))
        );

        // Check that the first field of the struct must be named "id".
        let first_field = match def.field(0) {
            Some(field) => field,
            None => {
                return Err(verification_failure(format!(
                    "First field of struct {} must be 'id', no field found",
                    name
                )))
            }
        };
        let first_field_name = view.identifier_at(first_field.name).as_str();
        fp_ensure!(
            first_field_name == "id",
            verification_failure(format!(
                "First field of struct {} must be 'id', {} found",
                name, first_field_name
            ))
        );
        // Check that the "id" field must have a struct type.
        let id_field_type = &first_field.signature.0;
        let id_field_type = match id_field_type {
            SignatureToken::Struct(struct_type) => struct_type,
            _ => {
                return Err(verification_failure(format!(
                    "First field of struct {} must be of ID type, {:?} type found",
                    name, id_field_type
                )))
            }
        };
        // Chech that the struct type for "id" field must be SUI_FRAMEWORK_ADDRESS::ID::ID.
        let id_type_struct = module.struct_handle_at(*id_field_type);
        let id_type_struct_name = view.identifier_at(id_type_struct.name).as_str();
        let id_type_module = module.module_handle_at(id_type_struct.module);
        let id_type_module_address = module.address_identifier_at(id_type_module.address);
        let id_type_module_name = module.identifier_at(id_type_module.name).to_string();
        fp_ensure!(
            id_type_struct_name == "VersionedID"
                && id_type_module_address == &SUI_FRAMEWORK_ADDRESS
                && id_type_module_name == "ID",
            verification_failure(format!(
                "First field of struct {} must be of type {}::ID::VersionedID, {}::{}::{} type found",
                name,
                SUI_FRAMEWORK_ADDRESS,
                id_type_module_address,
                id_type_module_name,
                id_type_struct_name
            ))
        );
    }
    Ok(())
}
