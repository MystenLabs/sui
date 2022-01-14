// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::adapter;
use anyhow::Result;
use fastx_framework::{self};
use fastx_types::{
    base_types::{FastPayAddress, TransactionDigest, TxContext},
    object::Object,
    FASTX_FRAMEWORK_ADDRESS, FASTX_FRAMEWORK_OBJECT_ID, MOVE_STDLIB_ADDRESS, MOVE_STDLIB_OBJECT_ID,
};
use move_vm_runtime::native_functions::NativeFunctionTable;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static GENESIS: Lazy<Mutex<Genesis>> =
    Lazy::new(|| Mutex::new(create_genesis_module_objects().unwrap()));

struct Genesis {
    pub objects: Vec<Object>,
    pub native_functions: NativeFunctionTable,
}

pub fn clone_genesis_data() -> (Vec<Object>, NativeFunctionTable) {
    let genesis = GENESIS.lock().unwrap();
    (genesis.objects.clone(), genesis.native_functions.clone())
}

/// Create and return objects wrapping the genesis modules for fastX
fn create_genesis_module_objects() -> Result<Genesis> {
    let mut tx_context = TxContext::new(TransactionDigest::genesis());
    let modules = fastx_framework::get_framework_packages()?;
    let packages = adapter::generate_package_info_map(modules, &mut tx_context)?;
    let fastx_framework_addr = packages[&FASTX_FRAMEWORK_ADDRESS].0;
    let move_stdlib_addr = packages[&MOVE_STDLIB_ADDRESS].0;
    if fastx_framework_addr != FASTX_FRAMEWORK_OBJECT_ID {
        panic!(
            "FastX framework address doesn't match, expecting: {:#X?}",
            fastx_framework_addr
        );
    }
    if move_stdlib_addr != MOVE_STDLIB_OBJECT_ID {
        panic!(
            "Move stdlib address doesn't match, expecting: {:#X?}",
            move_stdlib_addr
        );
    }
    let native_functions =
        fastx_framework::natives::all_natives(move_stdlib_addr, fastx_framework_addr);
    let owner = FastPayAddress::default();
    let objects = packages
        .into_values()
        .map(|(_, modules)| Object::new_package(modules, owner, TransactionDigest::genesis()))
        .collect();
    Ok(Genesis {
        objects,
        native_functions,
    })
}
