// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    metering::translation_meter::TranslationMeter, typing::ast as T,
};
use sui_types::{base_types::TxContextKind, error::ExecutionError};

/// After loading and type checking, we do a second pass over the typed transaction to charge for
/// type-related properties (before further analysis is done):
/// - number of type nodes (including nested)
/// - number of type references. These are charged non-linearly
pub fn meter(
    meter: &mut TranslationMeter,
    transaction: &T::Transaction,
) -> Result<(), ExecutionError> {
    let mut num_refs: u64 = 0;
    let mut num_nodes: u64 = 0;

    for ty in transaction.types() {
        if ty.is_reference() && ty.is_tx_context() == TxContextKind::None {
            num_refs = num_refs.saturating_add(1);
        }
        num_nodes = num_nodes.saturating_add(ty.node_count());
    }

    meter.charge_num_type_nodes(num_nodes)?;
    meter.charge_num_type_references(num_refs)?;
    Ok(())
}
