// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use once_cell::sync::Lazy;
use sui_types::base_types::{ObjectRef, SuiAddress, TxContext};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{base_types::TransactionDigest, object::Object};

static GENESIS: Lazy<Genesis> = Lazy::new(create_genesis_module_objects);

struct Genesis {
    pub objects: Vec<Object>,
    pub modules: Vec<Vec<CompiledModule>>,
}

pub fn clone_genesis_compiled_modules() -> Vec<Vec<CompiledModule>> {
    GENESIS.modules.clone()
}

pub fn clone_genesis_packages() -> Vec<Object> {
    GENESIS.objects.clone()
}

pub fn get_framework_object_ref() -> ObjectRef {
    GENESIS
        .objects
        .iter()
        .find(|o| o.id() == SUI_FRAMEWORK_ADDRESS.into())
        .unwrap()
        .compute_object_reference()
}

pub fn get_genesis_context() -> TxContext {
    TxContext::new(&SuiAddress::default(), &TransactionDigest::genesis())
}

/// Create and return objects wrapping the genesis modules for sui
fn create_genesis_module_objects() -> Genesis {
    let sui_modules = sui_framework::get_sui_framework();
    let std_modules = sui_framework::get_move_stdlib();
    let objects = vec![
        Object::new_package(std_modules.clone(), TransactionDigest::genesis()),
        Object::new_package(sui_modules.clone(), TransactionDigest::genesis()),
    ];
    let modules = vec![std_modules, sui_modules];
    Genesis { objects, modules }
}
