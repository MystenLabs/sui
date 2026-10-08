// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Forwarding addresses: the registration fee native, and the resolution that both the adapter
//! (at the end of every transaction) and `test_scenario` (at the end of every simulated
//! transaction) run over a transaction's outputs.
//!
//! Nothing can sign for a forwarding address, so anything left with one as its recipient would be
//! stranded. Every funds credit targeting a forwarding address is rerouted to the registered
//! master, with a `ForwardingDeposit` event recording the reroute for the issuer of the address.
//! Objects owned by a forwarding address fail the transaction instead; rerouting them, and
//! resolving a master that is itself a forwarding address, are not supported yet.

use std::collections::{BTreeMap, VecDeque};

use move_binary_format::{errors::PartialVMResult, safe_assert_eq, safe_unwrap};
use move_core_types::{
    gas_algebra::InternalGas,
    language_storage::{StructTag, TypeTag},
};
use move_vm_runtime::{
    execution::values::Value,
    native_charge_gas_early_exit,
    natives::functions::{NativeContext, NativeResult},
};
use smallvec::smallvec;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    accumulator_root::{
        AccumulatorValue, derive_event_stream_head_object_id, event_stream_head_type_tag,
    },
    balance::Balance,
    base_types::{ObjectID, SuiAddress},
    forwarding_address::{ForwardingAddress, ForwardingDeposit, ForwardingMaster},
    object::Owner,
    storage::ImplicitSystemObjectResolver,
};

use crate::{
    NativesCostTable, get_extension,
    object_runtime::{MoveAccumulatorAction, MoveAccumulatorEvent, MoveAccumulatorValue},
};

#[derive(Clone)]
pub struct ForwardingAddressRegisterCostParams {
    pub base: Option<InternalGas>,
}

/// Charges the registration price, which exists to make allocating master IDs expensive.
pub fn charge_registration_fee(
    context: &mut NativeContext,
    ty_args: Vec<move_vm_runtime::execution::Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 0);
    safe_assert_eq!(args.len(), 0);

    let ForwardingAddressRegisterCostParams { base } = get_extension!(context, NativesCostTable)?
        .forwarding_address_register_cost_params
        .clone();
    native_charge_gas_early_exit!(context, safe_unwrap!(base));

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

/// Where the gas for resolution goes. The adapter charges the transaction; `test_scenario`
/// charges nothing.
pub trait ForwardingGas {
    /// Once per forwarding address, when its resolution starts.
    fn charge_resolution(&mut self) -> Result<(), RerouteError>;
    /// Before every registry read.
    fn charge_lookup(&mut self) -> Result<(), RerouteError>;
    /// For every event emitted on Move's behalf, sized like `event::emit` sizes it.
    fn charge_event(&mut self, tag_size: u64, value_size: u64) -> Result<(), RerouteError>;
    /// For every event added to an authenticated event stream, like `event::emit_authenticated`.
    fn charge_event_stream(&mut self) -> Result<(), RerouteError>;
}

pub struct NoForwardingGas;

impl ForwardingGas for NoForwardingGas {
    fn charge_resolution(&mut self) -> Result<(), RerouteError> {
        Ok(())
    }
    fn charge_lookup(&mut self) -> Result<(), RerouteError> {
        Ok(())
    }
    fn charge_event(&mut self, _: u64, _: u64) -> Result<(), RerouteError> {
        Ok(())
    }
    fn charge_event_stream(&mut self) -> Result<(), RerouteError> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum RerouteError {
    /// A recipient cannot be resolved (unregistered id, unsupported variant, a master that is
    /// itself a forwarding address, an object owned by a forwarding address). The transaction
    /// fails with this message.
    Unresolvable(String),
    /// The registry could not be read at the version assigned to the transaction.
    InvariantViolation(String),
    OutOfGas,
}

/// Resolves forwarding addresses over one transaction's outputs. Each address is charged and read
/// once per call.
pub struct Resolver<'a> {
    protocol_config: &'a ProtocolConfig,
    registry: &'a dyn ImplicitSystemObjectResolver,
    gas: &'a mut dyn ForwardingGas,
    masters: BTreeMap<SuiAddress, SuiAddress>,
}

impl<'a> Resolver<'a> {
    pub fn new(
        protocol_config: &'a ProtocolConfig,
        registry: &'a dyn ImplicitSystemObjectResolver,
        gas: &'a mut dyn ForwardingGas,
    ) -> Self {
        Self {
            protocol_config,
            registry,
            gas,
            masters: BTreeMap::new(),
        }
    }

    /// The registered master of a forwarding address. The resolution and the registry read are
    /// charged before the read happens, so an unregistered id pays for its lookup too.
    fn master(&mut self, forwarding_address: SuiAddress) -> Result<SuiAddress, RerouteError> {
        if let Some(master) = self.masters.get(&forwarding_address) {
            return Ok(*master);
        }
        let Some(parsed) = ForwardingAddress::parse(forwarding_address) else {
            return Err(RerouteError::InvariantViolation(format!(
                "{forwarding_address} is not a forwarding address"
            )));
        };
        self.gas.charge_resolution()?;
        let max_variant = self.protocol_config.forwarding_address_max_variant();
        if u64::from(parsed.variant) > max_variant {
            return Err(RerouteError::Unresolvable(format!(
                "Forwarding address {forwarding_address} has variant {}, above the supported {max_variant}",
                parsed.variant
            )));
        }
        self.gas.charge_lookup()?;
        let master = self
            .registry
            .forwarding_master(parsed.master_id)
            .map_err(|err| {
                RerouteError::InvariantViolation(format!(
                    "Failed to load forwarding master record {}: {err}",
                    parsed.master_id
                ))
            })?;
        let Some(ForwardingMaster { master, paused }) = master else {
            return Err(RerouteError::Unresolvable(format!(
                "Forwarding address {forwarding_address} has no registered master (id {})",
                parsed.master_id
            )));
        };
        if paused {
            return Err(RerouteError::Unresolvable(format!(
                "Forwarding address {forwarding_address} is paused (id {})",
                parsed.master_id
            )));
        }
        // FIXME(forwarding-addresses): before this reaches production, follow the chain when the
        // master is itself a forwarding address, bounded by a protocol config hop limit, charging
        // each hop's lookup and emitting an event per hop.
        if ForwardingAddress::has_magic(master) {
            return Err(RerouteError::Unresolvable(format!(
                "Forwarding address {forwarding_address} resolves to another forwarding address {master}"
            )));
        }
        self.masters.insert(forwarding_address, master);
        Ok(master)
    }

    /// Reroutes every funds credit targeting a forwarding address to its master and returns the
    /// `ForwardingDeposit` events recording the reroutes, as type tag and BCS contents. Fails if
    /// any written object is owned by a forwarding address.
    ///
    /// Emits one event per forwarding address and coin type, carrying the sum of its credits, so
    /// the event states what the transaction paid to that address regardless of how many
    /// commands made up the payment, and a payment split across many credits cannot run into
    /// `max_num_event_emit`.
    ///
    /// With `forwarding_deposit_event_streams`, each event is also added to the authenticated
    /// event stream keyed by its master, so the master can verify every forwarded deposit with a
    /// light client. The returned events must be appended right after the transaction's
    /// `num_prior_events` Move events, since the stream entries pushed onto `credits` refer to
    /// them by index.
    pub fn reroute<'b>(
        &mut self,
        owners: impl Iterator<Item = (ObjectID, &'b Owner)>,
        credits: &mut Vec<MoveAccumulatorEvent>,
        num_prior_events: u64,
    ) -> Result<Vec<(StructTag, Vec<u8>)>, RerouteError> {
        // FIXME(forwarding-addresses): before this reaches production, reroute objects (coins,
        // NFTs) owned by a forwarding address to the master instead of failing, with an event
        // naming the forwarding address, since effects alone would lose it. Rejecting keeps the
        // object from being stranded until then.
        for (id, owner) in owners {
            if let Ok(address) = owner.get_owner_address()
                && ForwardingAddress::has_magic(address)
            {
                return Err(RerouteError::Unresolvable(format!(
                    "Object {id} cannot be sent to forwarding address {address}"
                )));
            }
        }

        let mut deposits: BTreeMap<(SuiAddress, TypeTag), (SuiAddress, u64)> = BTreeMap::new();
        for credit in credits.iter_mut() {
            let target = SuiAddress::from(credit.target_addr);
            if !ForwardingAddress::has_magic(target) {
                continue;
            }
            let MoveAccumulatorValue::U64(amount) = credit.value else {
                return Err(RerouteError::Unresolvable(format!(
                    "Event stream {target} cannot be a forwarding address"
                )));
            };
            let Some(coin_type) = Balance::maybe_get_balance_type_param(&credit.target_ty) else {
                return Err(RerouteError::Unresolvable(format!(
                    "Funds of type {} cannot be sent to forwarding address {target}",
                    credit.target_ty
                )));
            };
            let master = self.master(target)?;
            let (_, total) = deposits.entry((target, coin_type)).or_insert((master, 0));
            // A coin's total supply fits in a u64, so real credits cannot overflow the sum.
            *total = total.checked_add(amount).ok_or_else(|| {
                RerouteError::InvariantViolation(format!(
                    "Deposits to forwarding address {target} overflow u64"
                ))
            })?;
            let Ok(accumulator_id) = AccumulatorValue::get_field_id(master, &credit.target_ty)
            else {
                return Err(RerouteError::InvariantViolation(
                    "Failed to compute accumulator field id for a forwarding master".to_owned(),
                ));
            };
            credit.target_addr = master.into();
            credit.accumulator_id = *accumulator_id.inner();
        }

        let event_streams = self.protocol_config.forwarding_deposit_event_streams();
        let mut events = vec![];
        for ((forwarding_address, coin_type), (master, amount)) in deposits {
            let tag = ForwardingDeposit::struct_tag(coin_type);
            let contents = bcs::to_bytes(&ForwardingDeposit {
                forwarding_address,
                master,
                amount,
            })
            .expect("serializing a fixed-size event cannot fail");
            self.gas.charge_event(
                u64::from(tag.abstract_size_for_gas_metering()),
                contents.len() as u64,
            )?;
            if event_streams {
                self.gas.charge_event_stream()?;
                let Ok(stream_head_id) = derive_event_stream_head_object_id(master) else {
                    return Err(RerouteError::InvariantViolation(
                        "Failed to compute the event stream head id for a forwarding master"
                            .to_owned(),
                    ));
                };
                credits.push(MoveAccumulatorEvent {
                    accumulator_id: stream_head_id,
                    action: MoveAccumulatorAction::Merge,
                    target_addr: master.into(),
                    target_ty: event_stream_head_type_tag(),
                    value: MoveAccumulatorValue::EventRef(num_prior_events + events.len() as u64),
                });
            }
            events.push((tag, contents));
        }
        Ok(events)
    }
}
