// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This pass verifies necessary properties for Move Objects, i.e. structs with the `key` ability.
//! The properties checked are
//! - The first field is named "id"
//! - The first field has type `sui::object::UID`

use crate::verification_failure;
use move_binary_format::file_format::{CompiledModule, SignatureToken};
use sui_types::{
    error::ExecutionError,
    fp_ensure,
    id::{OBJECT_MODULE_NAME, UID_STRUCT_NAME},
    SUI_FRAMEWORK_ADDRESS,
};

pub fn verify_module(module: &CompiledModule) -> Result<(), ExecutionError> {
    verify_key_structs(module)?;
    verify_no_key_enums(module)
}

fn verify_key_structs(module: &CompiledModule) -> Result<(), ExecutionError> {
    let struct_defs = &module.struct_defs;
    for def in struct_defs {
        let handle = module.datatype_handle_at(def.struct_handle);
        if !handle.abilities.has_key() {
            continue;
        }
        let name = module.identifier_at(handle.name);

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
        let first_field_name = module.identifier_at(first_field.name).as_str();
        if first_field_name != "id" {
            return Err(verification_failure(format!(
                "First field of struct {} must be 'id', {} found",
                name, first_field_name
            )));
        }
        // Check that the "id" field must have a struct type.
        let uid_field_type = &first_field.signature.0;
        let uid_field_type = match uid_field_type {
            SignatureToken::Datatype(struct_type) => struct_type,
            _ => {
                return Err(verification_failure(format!(
                    "First field of struct {} must be of type {}::object::UID, \
                    {:?} type found",
                    name, SUI_FRAMEWORK_ADDRESS, uid_field_type
                )))
            }
        };
        // check that the struct type for "id" field must be SUI_FRAMEWORK_ADDRESS::object::UID.
        let uid_type_struct = module.datatype_handle_at(*uid_field_type);
        let uid_type_struct_name = module.identifier_at(uid_type_struct.name);
        let uid_type_module = module.module_handle_at(uid_type_struct.module);
        let uid_type_module_address = module.address_identifier_at(uid_type_module.address);
        let uid_type_module_name = module.identifier_at(uid_type_module.name);
        fp_ensure!(
            uid_type_struct_name == UID_STRUCT_NAME
                && uid_type_module_address == &SUI_FRAMEWORK_ADDRESS
                && uid_type_module_name == OBJECT_MODULE_NAME,
            verification_failure(format!(
                "First field of struct {} must be of type {}::object::UID, \
                {}::{}::{} type found",
                name,
                SUI_FRAMEWORK_ADDRESS,
                uid_type_module_address,
                uid_type_module_name,
                uid_type_struct_name
            ))
        );
    }
    Ok(())
}

fn verify_no_key_enums(module: &CompiledModule) -> Result<(), ExecutionError> {
    for def in &module.enum_defs {
        let handle = module.datatype_handle_at(def.enum_handle);
        if handle.abilities.has_key() {
            return Err(verification_failure(format!(
                "Enum {} cannot have the 'key' ability. Enums cannot have the 'key' ability.",
                module.identifier_at(handle.name)
            )));
        }
    }
    Ok(())
}
