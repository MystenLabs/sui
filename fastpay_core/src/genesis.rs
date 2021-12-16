// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastx_adapter::adapter;
use fastx_framework::{self};
use fastx_types::{
    base_types::{PublicKeyBytes, SequenceNumber, TransactionDigest, TxContext},
    object::Object,
    FASTX_FRAMEWORK_ADDRESS,
};

/// Create and return objects wrapping the genesis modules for fastX
pub fn create_genesis_module_objects() -> Result<Vec<Object>> {
    let mut tx_context = TxContext::new(TransactionDigest::genesis());
    let mut modules = fastx_framework::get_framework_modules()?;
    adapter::generate_module_ids(&mut modules, &mut tx_context)?;
    let module_objects = modules
        .into_iter()
        .map(|m| {
            Object::new_module(
                m,
                PublicKeyBytes::from_move_address_hack(&FASTX_FRAMEWORK_ADDRESS),
                SequenceNumber::new(),
            )
        })
        .collect();
    Ok(module_objects)
}
