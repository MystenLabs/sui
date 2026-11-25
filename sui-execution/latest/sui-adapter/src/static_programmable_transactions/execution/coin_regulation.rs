// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Enforces `sui::coin_regulation` pauses and address freezes on an executed PTB.
//!
//! State is read at the FreezeList version assigned to the transaction, so a transaction never sees
//! its own FreezeList writes. The reads are not charged for gas, and the per-address reads are
//! capped by `max_freeze_list_address_reads`.

use crate::{error::convert_vm_error_impl, execution_value::ExecutionState};
use move_binary_format::{errors::Location, partial_vm_error};
use std::collections::{BTreeMap, BTreeSet};
use sui_move_natives::object_runtime::BalanceTracking;
use sui_protocol_config::{LimitThresholdCrossed, ProtocolConfig, check_limit_by_meter};
use sui_types::{
    TypeTag,
    base_types::{SequenceNumber, SuiAddress},
    coin_regulation::{
        coin_type_name, get_coin_type_state, has_coin_type_state, is_address_frozen,
    },
    error::{ExecutionError, SuiError, VMMemoryLimitExceededSubStatusCode},
    execution_status::ExecutionErrorKind,
    gas_coin::GAS,
    metrics::LimitsMetrics,
    storage::RuntimeObjectResolver,
};

/// Fails if the transaction touches a paused coin type, or a coin type for which an involved
/// address is frozen.
pub(crate) fn check(
    state_view: &dyn ExecutionState,
    tracking: &BalanceTracking,
    protocol_config: &ProtocolConfig,
    metrics: &LimitsMetrics,
) -> Result<(), ExecutionError> {
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
        protocol_config,
        metrics,
    )
}

/// Fails with `MEMORY_LIMIT_EXCEEDED` if `reads` reaches `max_freeze_list_address_reads`.
pub(crate) fn check_address_reads(
    protocol_config: &ProtocolConfig,
    metrics: &LimitsMetrics,
    reads: u64,
) -> Result<(), ExecutionError> {
    let limit = protocol_config.max_freeze_list_address_reads();
    // Only user transactions are checked, so there is no separate system transaction limit.
    if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
        true,
        reads,
        limit,
        limit,
        metrics.excessive_freeze_list_address_reads
    ) {
        let error = partial_vm_error!(
            MEMORY_LIMIT_EXCEEDED,
            "Reading the FreezeList for {} or more addresses is not allowed",
            lim
        )
        .with_sub_status(
            VMMemoryLimitExceededSubStatusCode::FREEZE_LIST_ADDRESS_READ_LIMIT_EXCEEDED as u64,
        )
        .finish(Location::Undefined);
        // An error without a module location needs no module or function resolution.
        return Err(convert_vm_error_impl(error, &|id| id.clone(), &|_, _| None));
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
pub(crate) fn check_touched_types(
    resolver: &dyn RuntimeObjectResolver,
    freeze_list_version: SequenceNumber,
    touched: &BTreeSet<&TypeTag>,
    addresses: &BTreeSet<SuiAddress>,
    protocol_config: &ProtocolConfig,
    metrics: &LimitsMetrics,
) -> Result<(), ExecutionError> {
    let mut with_frozen_addresses = vec![];
    for coin_type in touched {
        let Some(state) =
            get_coin_type_state(resolver, freeze_list_version, coin_type).map_err(read_error)?
        else {
            continue;
        };
        if state.paused {
            return Err(ExecutionError::from_kind(
                ExecutionErrorKind::CoinTypeGlobalPause {
                    coin_type: coin_type_name(coin_type),
                },
            ));
        }
        if state.num_frozen_addresses > 0 {
            with_frozen_addresses.push(*coin_type);
        }
    }
    if with_frozen_addresses.is_empty() {
        return Ok(());
    }
    let reads = (with_frozen_addresses.len() as u64).saturating_mul(addresses.len() as u64);
    check_address_reads(protocol_config, metrics, reads)?;
    for coin_type in with_frozen_addresses {
        for address in addresses {
            if is_address_frozen(resolver, freeze_list_version, coin_type, *address)
                .map_err(read_error)?
            {
                return Err(ExecutionError::from_kind(
                    ExecutionErrorKind::AddressDeniedForCoin {
                        address: *address,
                        coin_type: coin_type_name(coin_type),
                    },
                ));
            }
        }
    }
    Ok(())
}

/// Drops from `coin_owners` the coin types migrated from the legacy deny list as of the assigned
/// FreezeList version. A migrated type is regulated only by the FreezeList, so its deny list
/// entries no longer apply.
///
/// The FreezeList is loaded, and so recorded in effects, whenever a non-SUI type is present.
pub(crate) fn remove_migrated_coin_types(
    state_view: &dyn ExecutionState,
    coin_owners: BTreeMap<TypeTag, BTreeSet<SuiAddress>>,
) -> Result<BTreeMap<TypeTag, BTreeSet<SuiAddress>>, ExecutionError> {
    if coin_owners.keys().all(GAS::is_gas_type) {
        return Ok(coin_owners);
    }
    let Some(version) = state_view.freeze_list_version() else {
        return Ok(coin_owners);
    };
    coin_owners
        .into_iter()
        .map(|(coin_type, owners)| {
            let migrated = !GAS::is_gas_type(&coin_type)
                && has_coin_type_state(state_view, version, &coin_type).map_err(read_error)?;
            Ok((!migrated).then_some((coin_type, owners)))
        })
        .filter_map(Result::transpose)
        .collect()
}

fn read_error(error: SuiError) -> ExecutionError {
    ExecutionError::invariant_violation(format!("failed to read the FreezeList: {error}"))
}
