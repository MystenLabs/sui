// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This pass verifies necessary properties for Move Objects, i.e. structs with the `key` ability.
//! The properties checked are
//! - The first field is named "info"
//! - The first field has type `sui::object::Info`

use crate::verification_failure;
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{CompiledModule, SignatureToken},
};
use sui_types::{
    error::ExecutionError,
    fp_ensure,
    id::{INFO_STRUCT_NAME, OBJECT_MODULE_NAME},
    SUI_FRAMEWORK_ADDRESS,
};

pub fn verify_module(module: &CompiledModule) -> Result<(), ExecutionError> {
    verify_key_structs(module)
}

fn verify_key_structs(module: &CompiledModule) -> Result<(), ExecutionError> {
    let view = BinaryIndexedView::Module(module);
    let struct_defs = &module.struct_defs;
    for def in struct_defs {
        let handle = module.struct_handle_at(def.struct_handle);
        if !handle.abilities.has_key() {
            continue;
        }
        let name = view.identifier_at(handle.name);

        // Check that the first field of the struct must be named "info".
        let first_field = match def.field(0) {
            Some(field) => field,
            None => {
                return Err(verification_failure(format!(
                    "First field of struct {} must be 'info', no field found",
                    name
                )))
            }
        };
        let first_field_name = view.identifier_at(first_field.name).as_str();
        if first_field_name != "info" {
            return Err(verification_failure(format!(
                "First field of struct {} must be 'info', {} found",
                name, first_field_name
            )));
        }
        // Check that the "info" field must have a struct type.
        let info_field_type = &first_field.signature.0;
        let info_field_type = match info_field_type {
            SignatureToken::Struct(struct_type) => struct_type,
            _ => {
                return Err(verification_failure(format!(
                    "First field of struct {} must be of type {}::object::Info, \
                    {:?} type found",
                    name, SUI_FRAMEWORK_ADDRESS, info_field_type
                )))
            }
        };
        // check that the struct type for "info" field must be SUI_FRAMEWORK_ADDRESS::object::Info.
        let info_type_struct = module.struct_handle_at(*info_field_type);
        let info_type_struct_name = view.identifier_at(info_type_struct.name);
        let info_type_module = module.module_handle_at(info_type_struct.module);
        let info_type_module_address = module.address_identifier_at(info_type_module.address);
        let info_type_module_name = module.identifier_at(info_type_module.name);
        fp_ensure!(
            info_type_struct_name == INFO_STRUCT_NAME
                && info_type_module_address == &SUI_FRAMEWORK_ADDRESS
                && info_type_module_name == OBJECT_MODULE_NAME,
            verification_failure(format!(
                "First field of struct {} must be of type {}::object::Info, \
                {}::{}::{} type found",
                name,
                SUI_FRAMEWORK_ADDRESS,
                info_type_module_address,
                info_type_module_name,
                info_type_struct_name
            ))
        );
    }
    Ok(())
}
