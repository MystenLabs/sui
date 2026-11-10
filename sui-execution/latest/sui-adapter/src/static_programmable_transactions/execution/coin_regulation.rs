// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Enforces `sui::coin_regulation` pauses and address freezes on an executed PTB.
//!
//! State is read at the FreezeList version assigned to the transaction, so a transaction never sees
//! its own FreezeList writes. The reads are not charged for gas, and the per-address reads are
//! capped by `max_freeze_list_address_reads`.

use crate::execution_value::ExecutionState;
use move_binary_format::{errors::PartialVMResult, partial_vm_error};
use std::collections::{BTreeMap, BTreeSet};
use sui_move_natives::object_runtime::BalanceTracking;
use sui_protocol_config::{LimitThresholdCrossed, ProtocolConfig, check_limit_by_meter};
use sui_types::{
    TypeTag,
    base_types::{SequenceNumber, SuiAddress},
    coin_regulation::{
        coin_type_name, get_coin_type_state, get_frozen_address_state, has_coin_type_state,
    },
    error::{ExecutionError, SuiError, VMMemoryLimitExceededSubStatusCode},
    execution_status::ExecutionErrorKind,
    gas_coin::GAS,
    metrics::LimitsMetrics,
    storage::RuntimeObjectResolver,
};

/// Fails if the transaction touches a paused coin type, or a coin type for which an involved
/// address is frozen. `check_reads` is given the number of per-address reads before any is made.
pub(crate) fn check<E: From<ExecutionError>>(
    state_view: &dyn ExecutionState,
    tracking: &BalanceTracking,
    check_reads: impl FnOnce(u64) -> Result<(), E>,
) -> Result<(), E> {
    let touched = touched_coin_types(tracking);
    if touched.is_empty() {
        return Ok(());
    }
    // Replay from effects assigns no version when the original execution never loaded it.
    let Some(freeze_list_version) = state_view.freeze_list_version() else {
        return Ok(());
    };
    check_touched_types(
        state_view,
        freeze_list_version,
        &touched,
        &tracking.addresses,
        check_reads,
    )
}

/// Fails with `MEMORY_LIMIT_EXCEEDED` if `reads` reaches `max_freeze_list_address_reads`.
pub(crate) fn check_address_reads(
    protocol_config: &ProtocolConfig,
    metrics: &LimitsMetrics,
    reads: u64,
) -> PartialVMResult<()> {
    let limit = protocol_config.max_freeze_list_address_reads();
    // Only user transactions are checked, so there is no separate system transaction limit.
    if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
        true,
        reads,
        limit,
        limit,
        metrics.excessive_freeze_list_address_reads
    ) {
        return Err(partial_vm_error!(
            MEMORY_LIMIT_EXCEEDED,
            "Reading the FreezeList for {} or more addresses is not allowed",
            lim
        )
        .with_sub_status(
            VMMemoryLimitExceededSubStatusCode::FREEZE_LIST_ADDRESS_READ_LIMIT_EXCEEDED as u64,
        ));
    }
    Ok(())
}

/// The non-SUI coin types with a nonzero total before or after execution.
pub(crate) fn touched_coin_types(tracking: &BalanceTracking) -> BTreeSet<&TypeTag> {
    tracking
        .before
        .values()
        .chain(tracking.after.values())
        .flatten()
        .filter(|(type_, total)| **total != 0 && !GAS::is_gas_type(type_))
        .map(|(type_, _)| &**type_)
        .collect()
}

/// Checks every touched type for a pause before checking any for a frozen address.
pub(crate) fn check_touched_types<E: From<ExecutionError>>(
    resolver: &dyn RuntimeObjectResolver,
    freeze_list_version: SequenceNumber,
    touched: &BTreeSet<&TypeTag>,
    addresses: &BTreeSet<SuiAddress>,
    check_reads: impl FnOnce(u64) -> Result<(), E>,
) -> Result<(), E> {
    let mut with_frozen_addresses = vec![];
    for coin_type in touched {
        let Some(state) =
            get_coin_type_state(resolver, freeze_list_version, coin_type).map_err(read_error)?
        else {
            continue;
        };
        if state.paused {
            return Err(
                ExecutionError::from_kind(ExecutionErrorKind::CoinTypeGlobalPause {
                    coin_type: coin_type_name(coin_type),
                })
                .into(),
            );
        }
        if state.num_frozen_addresses > 0 {
            with_frozen_addresses.push(*coin_type);
        }
    }
    if with_frozen_addresses.is_empty() {
        return Ok(());
    }
    let reads = (with_frozen_addresses.len() as u64).saturating_mul(addresses.len() as u64);
    check_reads(reads)?;
    for coin_type in with_frozen_addresses {
        for address in addresses {
            let frozen =
                get_frozen_address_state(resolver, freeze_list_version, coin_type, *address)
                    .map_err(read_error)?;
            if frozen.is_some() {
                return Err(
                    ExecutionError::from_kind(ExecutionErrorKind::AddressDeniedForCoin {
                        address: *address,
                        coin_type: coin_type_name(coin_type),
                    })
                    .into(),
                );
            }
        }
    }
    Ok(())
}

/// Removes from `coin_owners` the coin types with FreezeList state at the assigned version, which
/// the legacy deny list ignores.
///
/// The FreezeList is loaded, and so recorded in effects, whenever a non-SUI type is present.
pub(crate) fn remove_migrated_coin_types(
    state_view: &dyn ExecutionState,
    coin_owners: &mut BTreeMap<TypeTag, BTreeSet<SuiAddress>>,
) -> Result<(), ExecutionError> {
    if coin_owners.keys().all(GAS::is_gas_type) {
        return Ok(());
    }
    let Some(version) = state_view.freeze_list_version() else {
        return Ok(());
    };
    let mut migrated = vec![];
    for coin_type in coin_owners.keys().filter(|t| !GAS::is_gas_type(t)) {
        if has_coin_type_state(state_view, version, coin_type).map_err(read_error)? {
            migrated.push(coin_type.clone());
        }
    }
    for coin_type in migrated {
        coin_owners.remove(&coin_type);
    }
    Ok(())
}

fn read_error(error: SuiError) -> ExecutionError {
    ExecutionError::invariant_violation(format!("failed to read the FreezeList: {error}"))
}
