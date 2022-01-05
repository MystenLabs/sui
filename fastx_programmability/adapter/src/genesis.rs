// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::adapter;
use anyhow::Result;
use fastx_framework::{self};
use fastx_types::{
    base_types::{
        FastPayAddress, SequenceNumber, TransactionDigest, TxContext, TX_CONTEXT_ADDRESS,
        TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME,
    },
    coin::{COIN_ADDRESS, COIN_MODULE_NAME, COIN_STRUCT_NAME},
    gas_coin::{GAS_ADDRESS, GAS_MODULE_NAME, GAS_STRUCT_NAME},
    id::{ID_ADDRESS, ID_MODULE_NAME, ID_STRUCT_NAME},
    object::Object,
    FASTX_FRAMEWORK_ADDRESS, MOVE_STDLIB_ADDRESS,
};
use move_binary_format::access::ModuleAccess;
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::ModuleId,
};
use move_vm_runtime::native_functions::NativeFunctionTable;
use once_cell::sync::Lazy;
use std::{collections::BTreeMap, sync::Mutex};

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
            // substitution should not change module names
            assert_eq!(native.1, new_id.name().to_owned());
        }
    }
    let owner = FastPayAddress::default();
    let expected_addresses: BTreeMap<&IdentStr, (AccountAddress, &IdentStr)> = vec![
        (COIN_MODULE_NAME, (COIN_ADDRESS, COIN_STRUCT_NAME)),
        (GAS_MODULE_NAME, (GAS_ADDRESS, GAS_STRUCT_NAME)),
        (ID_MODULE_NAME, (ID_ADDRESS, ID_STRUCT_NAME)),
        (
            TX_CONTEXT_MODULE_NAME,
            (TX_CONTEXT_ADDRESS, TX_CONTEXT_STRUCT_NAME),
        ),
    ]
    .into_iter()
    .collect();
    let objects = modules
        .into_iter()
        .map(|m| {
            let self_id = m.self_id();
            // check that modules the runtime needs to know about have the expected names and addresses
            // if these assertions fail, it's likely because the corresponding constants need to be updated
            if let Some((address, struct_name)) = expected_addresses.get(self_id.name()) {
                assert!(
                    self_id.address() == address,
                    "Found new address for {}: {}",
                    self_id.name(),
                    self_id.address()
                );
                assert_eq!(
                    m.identifier_at(m.struct_handle_at(m.struct_defs[0].struct_handle).name),
                    *struct_name
                );
            }
            Object::new_module(
                m,
                owner,
                SequenceNumber::new(),
                TransactionDigest::genesis(),
            )
        })
        .collect();
    Ok(Genesis {
        objects,
        native_functions,
    })
}
