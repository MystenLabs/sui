// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use fastx_framework::{self};
use fastx_types::{
    base_types::{Authenticator, SuiAddress, TransactionDigest},
    object::Object,
    FASTX_FRAMEWORK_ADDRESS, MOVE_STDLIB_ADDRESS,
};
use move_vm_runtime::native_functions::NativeFunctionTable;
use once_cell::sync::Lazy;
use std::sync::Mutex;

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
    let fastx_modules = fastx_framework::get_fastx_framework_modules();
    let std_modules = fastx_framework::get_move_stdlib_modules();
    let native_functions =
        fastx_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, FASTX_FRAMEWORK_ADDRESS);
    let owner = SuiAddress::default();
    let objects = vec![
        Object::new_package(
            fastx_modules,
            Authenticator::Address(owner),
            TransactionDigest::genesis(),
        ),
        Object::new_package(
            std_modules,
            Authenticator::Address(owner),
            TransactionDigest::genesis(),
        ),
    ];
    Genesis {
        objects,
        native_functions,
    }
}
