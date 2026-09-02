// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    static_programmable_transactions::{
        metering::translation_meter::TranslationMeter, typing::ast as T,
    },
};
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::TxContextKind;

mod live_references;

/// After loading and type checking, we do a second pass over the typed transaction to charge for
/// type-related properties (before further analysis is done):
/// - number of type nodes (including nested)
/// - number of type references. These are charged non-linearly
/// - number of references live at each command. These are charged non-linearly and limited, along
///   with the number of references returned by each command. See `live_references` module.
pub fn meter<Mode: ExecutionMode>(
    meter: &mut TranslationMeter,
    protocol_config: &ProtocolConfig,
    transaction: &T::Transaction,
) -> Result<(), Mode::Error> {
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
    if protocol_config
        .max_ptb_live_references_as_option()
        .is_some()
    {
        live_references::meter::<Mode::Error>(meter, protocol_config, transaction)?;
    }
    Ok(())
}
