// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_framework::types::base_types::SuiAddress;
use sui_indexer_alt_framework::types::effects::TransactionEffects;
use sui_indexer_alt_framework::types::object::Owner;

pub(crate) mod cp_bloom_blocks;
pub(crate) mod cp_blooms;
pub(crate) mod cp_digests;
pub(crate) mod cp_sequence_numbers;
pub(crate) mod ev_emit_mod;
pub(crate) mod ev_struct_inst;
pub(crate) mod kv_checkpoints;
pub(crate) mod kv_epoch_ends;
pub(crate) mod kv_epoch_starts;
pub(crate) mod kv_feature_flags;
pub(crate) mod kv_objects;
pub(crate) mod kv_packages;
pub(crate) mod kv_protocol_configs;
pub(crate) mod kv_transactions;
pub(crate) mod obj_versions;
pub(crate) mod sum_displays;
pub(crate) mod tx_affected_addresses;
pub(crate) mod tx_affected_objects;
pub(crate) mod tx_balance_changes;
pub(crate) mod tx_calls;
pub(crate) mod tx_digests;
pub(crate) mod tx_kinds;

/// The recipient addresses from changed objects in a transaction's effects.
///
/// Returns addresses from `AddressOwner` and `ConsensusAddressOwner` owners,
/// skipping other owner types.
pub(crate) fn affected_addresses(effects: &TransactionEffects) -> impl Iterator<Item = SuiAddress> {
    effects
        .all_changed_objects()
        .into_iter()
        .filter_map(|(_, owner, _)| match owner {
            Owner::AddressOwner(address) => Some(address),
            Owner::ConsensusAddressOwner { owner, .. } => Some(owner),
            _ => None,
        })
}
