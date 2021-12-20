// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::adapter;
use anyhow::Result;
use fastx_framework::{self};
use fastx_types::{
    base_types::{PublicKeyBytes, SequenceNumber, TransactionDigest, TxContext},
    object::Object,
    FASTX_FRAMEWORK_ADDRESS, MOVE_STDLIB_ADDRESS,
};
use move_binary_format::access::ModuleAccess;
use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::IdentStr, language_storage::ModuleId,
};
use move_vm_runtime::native_functions::NativeFunctionTable;
use once_cell::sync::Lazy;
use std::sync::Mutex;

/// 0x873707f730d18d3867cb77ec7c838c0b
pub const TX_CONTEXT_ADDRESS: AccountAddress = AccountAddress::new([
    0x87, 0x37, 0x07, 0xf7, 0x30, 0xd1, 0x8d, 0x38, 0x67, 0xcb, 0x77, 0xec, 0x7c, 0x83, 0x8c, 0x0b,
]);
pub const TX_CONTEXT_MODULE_NAME: &IdentStr = ident_str!("TxContext");
pub const TX_CONTEXT_STRUCT_NAME: &IdentStr = TX_CONTEXT_MODULE_NAME;

pub static GENESIS: Lazy<Mutex<Genesis>> =
    Lazy::new(|| Mutex::new(create_genesis_module_objects().unwrap()));

pub struct Genesis {
    pub objects: Vec<Object>,
    pub native_functions: NativeFunctionTable,
}

/// Create and return objects wrapping the genesis modules for fastX
fn create_genesis_module_objects() -> Result<Genesis> {
    let mut tx_context = TxContext::new(TransactionDigest::genesis());
    let mut modules = fastx_framework::get_framework_modules()?;
    let sub_map = adapter::generate_module_ids(&mut modules, &mut tx_context)?;
    let mut native_functions =
        fastx_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, FASTX_FRAMEWORK_ADDRESS);
    // Rewrite native function table to reflect address substitutions. Otherwise, natives will fail to link
    for native in native_functions.iter_mut() {
        let old_id = ModuleId::new(native.0, native.1.to_owned());
        if let Some(new_id) = sub_map.get(&old_id) {
            native.0 = *new_id.address();
            native.1 = new_id.name().to_owned();
        }
    }

    let objects = modules
        .into_iter()
        .map(|m| {
            let self_id = m.self_id();
            // check that modules the runtime needs to know about have the expected names and addresses
            // if these assertions fail, it's likely because approrpiate constants need to be updated
            if self_id.name() == TX_CONTEXT_MODULE_NAME {
                assert!(
                    self_id.address() == &TX_CONTEXT_ADDRESS,
                    "Found new address for TxContext: {}",
                    self_id.address()
                );
                assert!(
                    m.identifier_at(m.struct_handle_at(m.struct_defs[0].struct_handle).name)
                        == TX_CONTEXT_STRUCT_NAME
                );
            }
            Object::new_module(
                m,
                PublicKeyBytes::from_move_address_hack(&FASTX_FRAMEWORK_ADDRESS),
                SequenceNumber::new(),
            )
        })
        .collect();
    Ok(Genesis {
        objects,
        native_functions,
    })
}
