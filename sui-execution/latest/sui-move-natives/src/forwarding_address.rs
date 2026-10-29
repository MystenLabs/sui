// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Forwarding addresses: the registration fee native, and the resolution that both the adapter
//! (at the end of every transaction) and `test_scenario` (at the end of every simulated
//! transaction) run over a transaction's outputs.
//!
//! Nothing can sign for a forwarding address, so anything left with one as its recipient would be
//! stranded. Every funds credit targeting a forwarding address, and every coin owned by one, is
//! delivered to the master at the end of the chain of registered masters, with one
//! `ForwardingDeposit` per hop recording what passed through each forwarding address. A coin is
//! melted into a funds credit on the way, so a payment arrives in one form however it was sent.
//! Any other object owned by a forwarding address fails the transaction.

use std::collections::{BTreeMap, VecDeque, btree_map::Entry};

use move_binary_format::{errors::PartialVMResult, safe_assert_eq, safe_unwrap};
use move_core_types::{
    account_address::AccountAddress,
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
    object_runtime::{
        MoveAccumulatorAction, MoveAccumulatorEvent, MoveAccumulatorValue, ObjectRuntime,
    },
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

#[derive(Clone)]
pub struct ForwardingAddressVariantCostParams {
    pub base: Option<InternalGas>,
}

/// Whether the resolver accepts the variant of a forwarding address at this protocol version.
/// Move cannot read the protocol config, so the registry asks here before accepting a forwarding
/// address as a master.
pub fn variant_is_supported(
    context: &mut NativeContext,
    ty_args: Vec<move_vm_runtime::execution::Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 0);
    safe_assert_eq!(args.len(), 1);

    let ForwardingAddressVariantCostParams { base } = get_extension!(context, NativesCostTable)?
        .forwarding_address_variant_cost_params
        .clone();
    native_charge_gas_early_exit!(context, safe_unwrap!(base));

    let address = safe_unwrap!(args.pop_back()).value_as::<AccountAddress>()?;
    let max_variant = get_extension!(context, ObjectRuntime)?
        .protocol_config
        .forwarding_address_max_variant();
    let supported = ForwardingAddress::parse(address.into())
        .is_some_and(|parsed| u64::from(parsed.variant) <= max_variant);
    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::bool(supported)],
    ))
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
    /// A recipient cannot be resolved (unregistered or paused id, unsupported variant, a chain
    /// longer than the protocol allows, a non-coin object owned by a forwarding address). The
    /// transaction fails with this message.
    Unresolvable(String),
    /// The registry could not be read at the version assigned to the transaction.
    InvariantViolation(String),
    OutOfGas,
}

/// What the resolver needs to know about an object written by the transaction: who owns it and,
/// if it is a coin, its coin type and value.
pub struct WrittenObject<'b> {
    pub id: ObjectID,
    pub owner: &'b Owner,
    pub coin: Option<(TypeTag, u64)>,
}

/// The outcome of resolving one transaction's outputs.
pub struct Rerouted {
    /// Events to emit on Move's behalf, as type tag and BCS contents, in order.
    pub events: Vec<(StructTag, Vec<u8>)>,
    /// Coins owned by a forwarding address, whose value became a funds credit. The caller deletes
    /// them.
    pub melted: Vec<ObjectID>,
}

#[derive(Clone, Copy)]
struct Hop {
    forwarding_address: SuiAddress,
    master: SuiAddress,
}

/// Resolves forwarding addresses over one transaction's outputs. Each address is charged and read
/// once per call.
pub struct Resolver<'a> {
    protocol_config: &'a ProtocolConfig,
    registry: &'a dyn ImplicitSystemObjectResolver,
    gas: &'a mut dyn ForwardingGas,
    chains: BTreeMap<SuiAddress, Vec<Hop>>,
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
            chains: BTreeMap::new(),
        }
    }

    /// Delivers every coin owned by a forwarding address and every funds credit targeting one to
    /// the master at the end of its chain.
    ///
    /// A coin is melted: its value joins the funds credits as a credit to the forwarding address,
    /// and the caller deletes the object, so the master sees one address-balance credit however
    /// the payer sent the funds. Any other object owned by a forwarding address, the gas coin, and
    /// a party object that grants a forwarding address permissions fail the transaction.
    ///
    /// Funds get one `ForwardingDeposit` per forwarding address passed through and coin type,
    /// carrying the sum of everything that passed through that address in the transaction. The
    /// event states what the transaction paid through the address however many commands made up
    /// the payment, a payment split across many credits cannot run into `max_num_event_emit`, and
    /// an intermediate master of a chain sees what flowed through it.
    ///
    /// With `forwarding_deposit_event_streams`, each event is also added to the authenticated
    /// event stream keyed by that hop's master, so every party along a chain can verify the
    /// deposits into its own address with a light client. The returned events must be appended
    /// right after the transaction's `num_prior_events` Move events, since the stream entries
    /// pushed onto `credits` refer to them by index.
    pub fn reroute<'b>(
        &mut self,
        objects: impl Iterator<Item = WrittenObject<'b>>,
        credits: &mut Vec<MoveAccumulatorEvent>,
        num_prior_events: u64,
        gas_coin: Option<ObjectID>,
    ) -> Result<Rerouted, RerouteError> {
        let mut melted = vec![];
        for object in objects {
            let address = match object.owner {
                Owner::AddressOwner(address)
                | Owner::ConsensusAddressOwner { owner: address, .. } => *address,
                Owner::ObjectOwner(_) | Owner::Shared { .. } | Owner::Immutable => continue,
                Owner::Party { permissions, .. } => {
                    // A forwarding address can never act, so a permission granted to one is
                    // meaningless and almost certainly a mistake worth failing loudly on.
                    if permissions
                        .members()
                        .any(|member| ForwardingAddress::has_magic(*member))
                    {
                        return Err(RerouteError::Unresolvable(format!(
                            "Party object {} cannot grant a forwarding address permissions",
                            object.id
                        )));
                    }
                    continue;
                }
            };
            if !ForwardingAddress::has_magic(address) {
                continue;
            }
            if gas_coin == Some(object.id) {
                return Err(RerouteError::Unresolvable(format!(
                    "The gas coin cannot be sent to forwarding address {address}"
                )));
            }
            let Some((coin_type, amount)) = object.coin.clone() else {
                return Err(RerouteError::Unresolvable(format!(
                    "Object {} cannot be sent to forwarding address {address}: only coins and \
                     balances are forwarded",
                    object.id
                )));
            };
            let target_ty = Balance::type_tag(coin_type);
            let Ok(accumulator_id) = AccumulatorValue::get_field_id(address, &target_ty) else {
                return Err(RerouteError::InvariantViolation(
                    "Failed to compute accumulator field id for a melted coin".to_owned(),
                ));
            };
            credits.push(MoveAccumulatorEvent {
                accumulator_id: *accumulator_id.inner(),
                action: MoveAccumulatorAction::Merge,
                target_addr: address.into(),
                target_ty,
                value: MoveAccumulatorValue::U64(amount),
            });
            melted.push(object.id);
        }

        // (forwarding address, coin type) -> (its master, total that passed through it)
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
            let chain = self.chain(target)?.to_vec();
            for hop in &chain {
                let (_, total) = deposits
                    .entry((hop.forwarding_address, coin_type.clone()))
                    .or_insert((hop.master, 0));
                // A coin's total supply fits in a u64, so real credits cannot overflow the sum.
                *total = total.checked_add(amount).ok_or_else(|| {
                    RerouteError::InvariantViolation(format!(
                        "Deposits through forwarding address {} overflow u64",
                        hop.forwarding_address
                    ))
                })?;
            }
            let master = chain
                .last()
                .expect("an address with the magic has a chain")
                .master;
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
        Ok(Rerouted { events, melted })
    }

    /// The chain of registered masters from a forwarding address to the first ordinary address,
    /// resolved once per address per transaction.
    fn chain(&mut self, address: SuiAddress) -> Result<&[Hop], RerouteError> {
        match self.chains.entry(address) {
            Entry::Occupied(chain) => Ok(chain.into_mut()),
            Entry::Vacant(slot) => {
                let chain =
                    Self::resolve_chain(self.protocol_config, self.registry, self.gas, address)?;
                Ok(slot.insert(chain))
            }
        }
    }

    /// The resolution and every registry read are charged before the read happens, so an
    /// unregistered id, or an over-long chain, pays for each lookup it caused.
    fn resolve_chain(
        protocol_config: &ProtocolConfig,
        registry: &dyn ImplicitSystemObjectResolver,
        gas: &mut dyn ForwardingGas,
        address: SuiAddress,
    ) -> Result<Vec<Hop>, RerouteError> {
        if protocol_config.freeze_forwarding_addresses() {
            return Err(RerouteError::Unresolvable(format!(
                "Forwarding address {address} cannot be resolved while forwarding addresses are frozen"
            )));
        }
        let max_hops = protocol_config.forwarding_address_max_hops();
        let max_variant = protocol_config.forwarding_address_max_variant();
        let mut chain: Vec<Hop> = vec![];
        let mut current = address;
        while let Some(parsed) = ForwardingAddress::parse(current) {
            if chain.len() as u64 >= max_hops {
                return Err(RerouteError::Unresolvable(format!(
                    "Forwarding address {address} resolves through more than {max_hops} hops"
                )));
            }
            if chain.is_empty() {
                gas.charge_resolution()?;
            }
            if u64::from(parsed.variant) > max_variant {
                return Err(RerouteError::Unresolvable(format!(
                    "Forwarding address {current} has variant {}, above the supported {max_variant}",
                    parsed.variant
                )));
            }
            gas.charge_lookup()?;
            let master = registry
                .forwarding_master(parsed.master_id)
                .map_err(|err| {
                    RerouteError::InvariantViolation(format!(
                        "Failed to load forwarding master record {}: {err}",
                        parsed.master_id
                    ))
                })?;
            let Some(ForwardingMaster { master, paused }) = master else {
                return Err(RerouteError::Unresolvable(format!(
                    "Forwarding address {current} has no registered master (id {})",
                    parsed.master_id
                )));
            };
            if paused {
                return Err(RerouteError::Unresolvable(format!(
                    "Forwarding address {current} is paused (id {})",
                    parsed.master_id
                )));
            }
            chain.push(Hop {
                forwarding_address: current,
                master,
            });
            current = master;
        }
        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_runtime::MoveAccumulatorAction;
    use sui_types::{
        error::{SuiErrorKind, SuiResult},
        forwarding_address::FORWARDING_ADDRESS_PAYLOAD_LENGTH,
        gas_coin::GAS,
    };

    /// Registry contents by master id.
    struct Registry(BTreeMap<u64, SuiAddress>);

    impl ImplicitSystemObjectResolver for Registry {
        fn object_available_balance(&self, _: SuiAddress, _: &TypeTag) -> SuiResult<u128> {
            unreachable!("resolution never reads balances")
        }

        fn clock(&self) -> SuiResult<sui_types::object::Object> {
            unreachable!("resolution never reads the clock")
        }

        fn forwarding_master(&self, master_id: u64) -> SuiResult<Option<ForwardingMaster>> {
            Ok(self.0.get(&master_id).map(|master| ForwardingMaster {
                master: *master,
                paused: false,
            }))
        }
    }

    fn forwarding(master_id: u64, variant: u8) -> SuiAddress {
        ForwardingAddress::derive(master_id, variant, [7; FORWARDING_ADDRESS_PAYLOAD_LENGTH])
    }

    fn ordinary(byte: u8) -> SuiAddress {
        SuiAddress::from_bytes([byte; 32]).unwrap()
    }

    fn config() -> ProtocolConfig {
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        assert_eq!(config.forwarding_address_max_hops(), 3);
        // Streams are exercised end to end; here the credits are compared by value.
        config.set_forwarding_deposit_event_streams_for_testing(false);
        config
    }

    fn credit(target: SuiAddress, amount: u64) -> MoveAccumulatorEvent {
        let target_ty = Balance::type_tag(GAS::type_tag());
        MoveAccumulatorEvent {
            accumulator_id: *AccumulatorValue::get_field_id(target, &target_ty)
                .unwrap()
                .inner(),
            action: MoveAccumulatorAction::Merge,
            target_addr: target.into(),
            target_ty,
            value: MoveAccumulatorValue::U64(amount),
        }
    }

    fn decode_deposits(events: Vec<(StructTag, Vec<u8>)>) -> Vec<ForwardingDeposit> {
        events
            .into_iter()
            .map(|(tag, contents)| {
                assert_eq!(tag, ForwardingDeposit::struct_tag(GAS::type_tag()));
                bcs::from_bytes(&contents).unwrap()
            })
            .collect()
    }

    /// Reroutes `credits` with no objects and returns the deposit events decoded.
    fn reroute_credits(
        registry: &dyn ImplicitSystemObjectResolver,
        credits: &mut Vec<MoveAccumulatorEvent>,
    ) -> Result<Vec<ForwardingDeposit>, RerouteError> {
        let config = config();
        let rerouted = Resolver::new(&config, registry, &mut NoForwardingGas).reroute(
            std::iter::empty(),
            credits,
            0,
            None,
        )?;
        assert!(rerouted.melted.is_empty());
        Ok(decode_deposits(rerouted.events))
    }

    fn assert_unresolvable<T>(result: Result<T, RerouteError>, needle: &str) {
        match result {
            Err(RerouteError::Unresolvable(message)) => {
                assert!(message.contains(needle), "{message}")
            }
            Err(other) => {
                panic!("expected an unresolvable address containing {needle:?}: {other:?}")
            }
            Ok(_) => panic!("expected an unresolvable address containing {needle:?}, got Ok"),
        }
    }

    fn by_address(deposits: Vec<ForwardingDeposit>) -> BTreeMap<SuiAddress, (SuiAddress, u64)> {
        deposits
            .into_iter()
            .map(|d| (d.forwarding_address, (d.master, d.amount)))
            .collect()
    }

    #[test]
    fn chains_resolve_up_to_the_hop_bound() {
        // 4 -> 3 -> 2 -> 1 -> ordinary: three hops from id 3, four from id 4.
        let registry = Registry(BTreeMap::from([
            (1, ordinary(0xaa)),
            (2, forwarding(1, 0)),
            (3, forwarding(2, 0)),
            (4, forwarding(3, 0)),
        ]));
        let mut credits = vec![credit(forwarding(3, 0), 500)];
        let deposits = by_address(reroute_credits(&registry, &mut credits).unwrap());
        assert_eq!(SuiAddress::from(credits[0].target_addr), ordinary(0xaa));
        assert_eq!(deposits.len(), 3);
        assert_eq!(deposits[&forwarding(3, 0)], (forwarding(2, 0), 500));
        assert_eq!(deposits[&forwarding(2, 0)], (forwarding(1, 0), 500));
        assert_eq!(deposits[&forwarding(1, 0)], (ordinary(0xaa), 500));
        assert_unresolvable(
            reroute_credits(&registry, &mut vec![credit(forwarding(4, 0), 1)]),
            "more than 3 hops",
        );
    }

    #[test]
    fn deposits_through_a_shared_hop_are_summed_per_address() {
        // Two issuers whose masters are both forwarding addresses of one PSP:
        // 2 -> 1 -> ordinary and 3 -> 1 -> ordinary.
        let registry = Registry(BTreeMap::from([
            (1, ordinary(0xaa)),
            (2, forwarding(1, 0)),
            (3, forwarding(1, 0)),
        ]));
        let mut credits = vec![
            credit(forwarding(2, 0), 300),
            credit(forwarding(2, 0), 400),
            credit(forwarding(3, 0), 1000),
        ];
        let deposits = by_address(reroute_credits(&registry, &mut credits).unwrap());
        assert_eq!(deposits.len(), 3);
        assert_eq!(deposits[&forwarding(2, 0)], (forwarding(1, 0), 700));
        assert_eq!(deposits[&forwarding(3, 0)], (forwarding(1, 0), 1000));
        assert_eq!(deposits[&forwarding(1, 0)], (ordinary(0xaa), 1700));
        for credit in &credits {
            assert_eq!(SuiAddress::from(credit.target_addr), ordinary(0xaa));
        }
    }

    #[test]
    fn unregistered_and_unsupported_addresses_fail_at_their_hop() {
        let registry = Registry(BTreeMap::from([
            (1, forwarding(9, 0)),
            (2, forwarding(1, 1)),
        ]));
        assert_unresolvable(
            reroute_credits(&registry, &mut vec![credit(forwarding(9, 0), 1)]),
            "no registered master",
        );
        assert_unresolvable(
            reroute_credits(&registry, &mut vec![credit(forwarding(1, 0), 1)]),
            "no registered master (id 9)",
        );
        assert_unresolvable(
            reroute_credits(&registry, &mut vec![credit(forwarding(2, 0), 1)]),
            "variant 1",
        );
    }

    #[test]
    fn coins_owned_by_a_forwarding_address_are_melted_into_credits() {
        let registry = Registry(BTreeMap::from([(1, ordinary(0xaa))]));
        let coin_id = ObjectID::random();
        let owner = Owner::AddressOwner(forwarding(1, 0));
        let objects = [WrittenObject {
            id: coin_id,
            owner: &owner,
            coin: Some((GAS::type_tag(), 250)),
        }];
        let mut credits = vec![credit(forwarding(1, 0), 50)];
        let config = config();
        let rerouted = Resolver::new(&config, &registry, &mut NoForwardingGas)
            .reroute(objects.into_iter(), &mut credits, 0, None)
            .unwrap();
        assert_eq!(rerouted.melted, vec![coin_id]);
        // The melted coin became a second credit, and both now target the master.
        assert_eq!(credits.len(), 2);
        for credit in &credits {
            assert_eq!(SuiAddress::from(credit.target_addr), ordinary(0xaa));
        }
        assert!(matches!(credits[1].value, MoveAccumulatorValue::U64(250)));
        let deposits = by_address(decode_deposits(rerouted.events));
        assert_eq!(deposits[&forwarding(1, 0)], (ordinary(0xaa), 300));
    }

    #[test]
    fn other_objects_the_gas_coin_and_party_members_fail() {
        let registry = Registry(BTreeMap::from([(1, ordinary(0xaa))]));
        let config = config();
        let owner = Owner::AddressOwner(forwarding(1, 0));

        let object = [WrittenObject {
            id: ObjectID::random(),
            owner: &owner,
            coin: None,
        }];
        assert_unresolvable(
            Resolver::new(&config, &registry, &mut NoForwardingGas).reroute(
                object.into_iter(),
                &mut vec![],
                0,
                None,
            ),
            "only coins and balances are forwarded",
        );

        let gas_id = ObjectID::random();
        let gas = [WrittenObject {
            id: gas_id,
            owner: &owner,
            coin: Some((GAS::type_tag(), 1)),
        }];
        assert_unresolvable(
            Resolver::new(&config, &registry, &mut NoForwardingGas).reroute(
                gas.into_iter(),
                &mut vec![],
                0,
                Some(gas_id),
            ),
            "gas coin",
        );

        let party = Owner::Party {
            start_version: Default::default(),
            permissions: sui_types::object::Party::new(
                sui_types::object::ObjectPermissions::ALL,
                BTreeMap::from([(forwarding(1, 0), sui_types::object::ObjectPermissions::NONE)]),
            )
            .unwrap(),
        };
        let party_object = [WrittenObject {
            id: ObjectID::random(),
            owner: &party,
            coin: None,
        }];
        assert_unresolvable(
            Resolver::new(&config, &registry, &mut NoForwardingGas).reroute(
                party_object.into_iter(),
                &mut vec![],
                0,
                None,
            ),
            "permissions",
        );
    }

    #[test]
    fn registry_errors_are_invariant_violations() {
        struct Broken;
        impl ImplicitSystemObjectResolver for Broken {
            fn object_available_balance(&self, _: SuiAddress, _: &TypeTag) -> SuiResult<u128> {
                unreachable!()
            }
            fn clock(&self) -> SuiResult<sui_types::object::Object> {
                unreachable!()
            }
            fn forwarding_master(&self, _: u64) -> SuiResult<Option<ForwardingMaster>> {
                Err(SuiErrorKind::ExecutionInvariantViolation.into())
            }
        }
        assert!(matches!(
            reroute_credits(&Broken, &mut vec![credit(forwarding(1, 0), 1)]),
            Err(RerouteError::InvariantViolation(_))
        ));
    }
}
