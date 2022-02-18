// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_vm_runtime::native_functions::NativeFunctionTable;
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sui_framework::{self, DEFAULT_FRAMEWORK_PATH};
use sui_types::error::SuiResult;
use sui_types::{
    base_types::{SuiAddress, TransactionDigest},
    object::Object,
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};

static GENESIS: Lazy<Mutex<Genesis>> = Lazy::new(|| {
    Mutex::new(create_genesis_module_objects(&PathBuf::from(DEFAULT_FRAMEWORK_PATH)).unwrap())
});

struct Genesis {
    pub objects: Vec<Object>,
    pub native_functions: NativeFunctionTable,
}

pub fn clone_genesis_data() -> (Vec<Object>, NativeFunctionTable) {
    let genesis = GENESIS.lock().unwrap();
    (genesis.objects.clone(), genesis.native_functions.clone())
}

/// Create and return objects wrapping the genesis modules for fastX
fn create_genesis_module_objects(lib_dir: &Path) -> SuiResult<Genesis> {
    let sui_modules = sui_framework::get_sui_framework_modules(lib_dir)?;
    let std_modules =
        sui_framework::get_move_stdlib_modules(&lib_dir.join("deps").join("move-stdlib"))?;
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let owner = SuiAddress::default();
    let objects = vec![
        Object::new_package(sui_modules, owner, TransactionDigest::genesis()),
        Object::new_package(std_modules, owner, TransactionDigest::genesis()),
    ];
    Ok(Genesis {
        objects,
        native_functions,
    })
}
