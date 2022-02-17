// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_vm_runtime::native_functions::NativeFunctionTable;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use sui_framework::{self};
use sui_types::{
    base_types::{SuiAddress, TransactionDigest},
    object::Object,
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};

static GENESIS: Lazy<Mutex<Genesis>> = Lazy::new(|| Mutex::new(create_genesis_module_objects()));

struct Genesis {
    pub objects: Vec<Object>,
    pub native_functions: NativeFunctionTable,
}

pub fn clone_genesis_data() -> (Vec<Object>, NativeFunctionTable) {
    let genesis = GENESIS.lock().unwrap();
    (genesis.objects.clone(), genesis.native_functions.clone())
}

/// Create and return objects wrapping the genesis modules for fastX
fn create_genesis_module_objects() -> Genesis {
    let sui_modules = sui_framework::get_sui_framework_modules();
    let std_modules = sui_framework::get_move_stdlib_modules();
    let native_functions = sui_framework::natives::all_natives(
        move_core_types::account_address::AccountAddress::from(MOVE_STDLIB_ADDRESS),
        move_core_types::account_address::AccountAddress::from(SUI_FRAMEWORK_ADDRESS),
    );
    let owner = SuiAddress::default();
    let objects = vec![
        Object::new_package(sui_modules, owner, TransactionDigest::genesis()),
        Object::new_package(std_modules, owner, TransactionDigest::genesis()),
    ];
    Genesis {
        objects,
        native_functions,
    }
}
