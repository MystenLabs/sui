// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The adapter's side of forwarding-address resolution: the shared resolver in
//! `sui_move_natives::forwarding_address` does the work; this charges the transaction's gas,
//! turns the resulting events into `Event`s and maps failures to execution errors.

use std::collections::BTreeMap;

use sui_move_natives::{
    forwarding_address::{ForwardingGas, RerouteError, Resolver, WrittenObject},
    object_runtime::MoveAccumulatorEvent,
};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    SUI_FRAMEWORK_ADDRESS,
    base_types::{ObjectID, SuiAddress},
    coin::Coin,
    error::ExecutionError,
    event::Event,
    execution_status::{ExecutionErrorKind, MoveLocationOpt},
    forwarding_address::{FORWARDING_ADDRESS_MODULE_NAME, ForwardingAddress},
    object::Object,
    storage::ImplicitSystemObjectResolver,
};

use crate::gas_charger::GasCharger;

struct TransactionGas<'a> {
    protocol_config: &'a ProtocolConfig,
    gas_charger: &'a mut GasCharger,
}

impl ForwardingGas for TransactionGas<'_> {
    fn charge_resolution(&mut self) -> Result<(), RerouteError> {
        self.gas_charger
            .charge_forwarding_resolution(self.protocol_config)
            .map_err(|_| RerouteError::OutOfGas)
    }

    fn charge_lookup(&mut self) -> Result<(), RerouteError> {
        self.gas_charger
            .charge_forwarding_lookup(self.protocol_config)
            .map_err(|_| RerouteError::OutOfGas)
    }

    fn charge_event(&mut self, tag_size: u64, value_size: u64) -> Result<(), RerouteError> {
        self.gas_charger
            .charge_synthesized_event(self.protocol_config, tag_size, value_size)
            .map_err(|_| RerouteError::OutOfGas)
    }

    fn charge_event_stream(&mut self) -> Result<(), RerouteError> {
        self.gas_charger
            .charge_synthesized_event_stream(self.protocol_config)
            .map_err(|_| RerouteError::OutOfGas)
    }
}

fn execution_error(error: RerouteError) -> ExecutionError {
    match error {
        RerouteError::Unresolvable(message) => {
            ExecutionError::new_with_source(ExecutionErrorKind::FeatureNotYetSupported, message)
        }
        RerouteError::InvariantViolation(message) => ExecutionError::invariant_violation(message),
        RerouteError::OutOfGas => ExecutionError::from_kind(ExecutionErrorKind::InsufficientGas),
    }
}

/// Fails the transaction if the gas coin was sent with `send_funds` to a forwarding address.
///
/// Resolving it would need its own path: the budget refund and the gas charge location are set
/// from the gas coin's recipient before `reroute` runs, and the charge is applied after it, so
/// `reroute` cannot fix them up. Keeping `reroute` the only way funds reach a master is what lets
/// us reason about resolution in one place.
///
/// It would also break accounting. The master would receive the gas coin's value plus the
/// refunded budget, minus gas used, an amount known only after gas is charged, so no
/// `ForwardingDeposit` could state what was actually paid, and matching a payment against its
/// invoice would have to account for gas. A payer splits the amount off the gas coin and sends
/// that instead.
pub fn reject_gas_coin_recipient(
    protocol_config: &ProtocolConfig,
    recipient: SuiAddress,
) -> Result<(), ExecutionError> {
    if protocol_config.enable_forwarding_addresses() && ForwardingAddress::has_magic(recipient) {
        return Err(execution_error(RerouteError::Unresolvable(format!(
            "The gas coin cannot be sent to forwarding address {recipient}"
        ))));
    }
    Ok(())
}

/// The events the adapter emits on Move's behalf and the coins it deletes because their value
/// was forwarded as funds.
pub struct Rerouted {
    pub events: Vec<Event>,
    pub melted_coins: Vec<ObjectID>,
}

/// Delivers the transaction's funds credits and coins owned by forwarding addresses to their
/// masters. Fails if any other object is owned by a forwarding address.
pub fn reroute(
    protocol_config: &ProtocolConfig,
    registry: &dyn ImplicitSystemObjectResolver,
    gas_charger: &mut GasCharger,
    sender: SuiAddress,
    written_objects: &BTreeMap<ObjectID, Object>,
    accumulator_events: &mut Vec<MoveAccumulatorEvent>,
    num_prior_events: usize,
) -> Result<Rerouted, ExecutionError> {
    let gas_coin = gas_charger.gas_coin();
    let objects = written_objects
        .iter()
        .map(|(id, object)| {
            let coin = Coin::extract_balance_if_coin(object).map_err(|err| {
                ExecutionError::invariant_violation(format!("Failed to read coin {id}: {err}"))
            })?;
            Ok(WrittenObject {
                id: *id,
                owner: &object.owner,
                coin,
            })
        })
        .collect::<Result<Vec<_>, ExecutionError>>()?;
    let mut gas = TransactionGas {
        protocol_config,
        gas_charger,
    };
    let rerouted = Resolver::new(protocol_config, registry, &mut gas)
        .reroute(
            objects.into_iter(),
            accumulator_events,
            num_prior_events as u64,
            gas_coin,
        )
        .map_err(execution_error)?;
    Ok(Rerouted {
        events: rerouted
            .events
            .into_iter()
            .map(|(tag, contents)| {
                Event::new(
                    &SUI_FRAMEWORK_ADDRESS,
                    FORWARDING_ADDRESS_MODULE_NAME,
                    sender,
                    tag,
                    contents,
                )
            })
            .collect(),
        melted_coins: rerouted.melted,
    })
}

/// Events the adapter emits count against the same per-transaction limits as Move events: the
/// number of events, the size of each, and the total size including the `move_events_size`
/// that `event::emit` already metered. Sizes use the serialized contents where Move uses the
/// value's abstract size, as `charge_synthesized_event` does.
pub fn check_event_limits(
    protocol_config: &ProtocolConfig,
    num_move_events: usize,
    move_events_size: u64,
    events: &[Event],
) -> Result<(), ExecutionError> {
    let limit_error = |message: String| {
        ExecutionError::new_with_source(
            ExecutionErrorKind::MovePrimitiveRuntimeError(MoveLocationOpt(None)),
            message,
        )
    };
    let max_events = protocol_config.max_num_event_emit();
    if num_move_events.saturating_add(events.len()) as u64 > max_events {
        return Err(limit_error(format!(
            "Emitting more than {max_events} events is not allowed"
        )));
    }
    let max_event_size = protocol_config.max_event_emit_size();
    let mut total_size = move_events_size;
    for event in events {
        let size = u64::from(event.type_.abstract_size_for_gas_metering())
            .saturating_add(event.contents.len() as u64);
        if size > max_event_size {
            return Err(limit_error(format!(
                "Emitting event of size {size} bytes. Limit is {max_event_size} bytes."
            )));
        }
        total_size = total_size.saturating_add(size);
    }
    if let Some(max_total) = protocol_config.max_event_emit_size_total_as_option()
        && total_size > max_total
    {
        return Err(limit_error(format!(
            "Reached total event size of size {total_size} bytes. Limit is {max_total} bytes."
        )));
    }
    Ok(())
}
