// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Forwarding addresses: the registration fee native, and the resolution that both the adapter
//! (at the end of every transaction) and `test_scenario` (at the end of every simulated
//! transaction) run over a transaction's outputs.
//!
//! Nothing can sign for a forwarding address, so anything left with one as its recipient would be
//! stranded. Every written object owned by a forwarding address and every funds credit targeting
//! one is rerouted to the registered master, following the chain when the master is itself a
//! forwarding address, and events record what passed through each address on the way.

use std::collections::{BTreeMap, VecDeque, btree_map::Entry};

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
    accumulator_root::AccumulatorValue,
    balance::Balance,
    base_types::{ObjectID, SuiAddress},
    forwarding_address::{
        ForwardingAddress, ForwardingDeposit, ForwardingMaster, ForwardingTransfer,
    },
    object::Owner,
    storage::ImplicitSystemObjectResolver,
};

use crate::{
    NativesCostTable, get_extension,
    object_runtime::{MoveAccumulatorEvent, MoveAccumulatorValue},
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
}

#[derive(Debug)]
pub enum RerouteError {
    /// A recipient cannot be resolved (unregistered id, unsupported variant, more hops than the
    /// protocol allows, a party object granting a forwarding address permissions). The
    /// transaction fails with this message.
    Unresolvable(String),
    /// The registry could not be read at the version assigned to the transaction.
    InvariantViolation(String),
    OutOfGas,
}

/// One step of a resolution chain: what was sent to `forwarding_address` goes to `master`.
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

    /// Reroutes every written object owned by a forwarding address and every funds credit
    /// targeting one to the end of its chain, and returns the events recording the reroutes, as
    /// type tag and BCS contents.
    ///
    /// Funds get one `ForwardingDeposit` per forwarding address passed through and coin type,
    /// carrying the sum of everything that passed through that address in the transaction. The
    /// event states what the transaction paid through the address however many commands made up
    /// the payment, a payment split across many credits cannot run into `max_num_event_emit`, and
    /// an intermediate master of a chain sees what flowed through it. Objects get one
    /// `ForwardingTransfer` per hop.
    pub fn reroute<'b>(
        &mut self,
        owners: impl Iterator<Item = (ObjectID, &'b mut Owner)>,
        credits: &mut [MoveAccumulatorEvent],
    ) -> Result<Vec<(StructTag, Vec<u8>)>, RerouteError> {
        let mut events = vec![];
        for (id, owner) in owners {
            let address = match owner {
                Owner::AddressOwner(address)
                | Owner::ConsensusAddressOwner { owner: address, .. } => address,
                Owner::ObjectOwner(_) | Owner::Shared { .. } | Owner::Immutable => continue,
                Owner::Party { permissions, .. } => {
                    // A forwarding address can never act, so a permission granted to one is
                    // meaningless and almost certainly a mistake worth failing loudly on.
                    if permissions
                        .members()
                        .any(|member| ForwardingAddress::has_magic(*member))
                    {
                        return Err(RerouteError::Unresolvable(format!(
                            "Party object {id} cannot grant a forwarding address permissions"
                        )));
                    }
                    continue;
                }
            };
            if !ForwardingAddress::has_magic(*address) {
                continue;
            }
            let chain = self.chain(*address)?.to_vec();
            for hop in &chain {
                let event = ForwardingTransfer {
                    forwarding_address: hop.forwarding_address,
                    master: hop.master,
                    object_id: id,
                };
                events.push(self.event(
                    ForwardingTransfer::struct_tag(),
                    bcs::to_bytes(&event).expect("serializing a fixed-size event cannot fail"),
                )?);
            }
            *address = chain
                .last()
                .expect("an address with the magic has a chain")
                .master;
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
        for ((forwarding_address, coin_type), (master, amount)) in deposits {
            let event = ForwardingDeposit {
                forwarding_address,
                master,
                amount,
            };
            events.push(self.event(
                ForwardingDeposit::struct_tag(coin_type),
                bcs::to_bytes(&event).expect("serializing a fixed-size event cannot fail"),
            )?);
        }
        Ok(events)
    }

    /// The resolution chain of `address`, empty when it is an ordinary address. The resolution is
    /// charged once per address and each hop's registry read is charged before it happens, so
    /// unregistered ids and over-long chains pay for every lookup they caused.
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

    fn resolve_chain(
        protocol_config: &ProtocolConfig,
        registry: &dyn ImplicitSystemObjectResolver,
        gas: &mut dyn ForwardingGas,
        address: SuiAddress,
    ) -> Result<Vec<Hop>, RerouteError> {
        let max_hops = protocol_config.forwarding_address_max_hops();
        let max_variant = protocol_config.forwarding_address_max_variant();
        let mut chain = vec![];
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

    /// An event emitted on Move's behalf, charged like `event::emit`.
    fn event(
        &mut self,
        tag: StructTag,
        contents: Vec<u8>,
    ) -> Result<(StructTag, Vec<u8>), RerouteError> {
        self.gas.charge_event(
            u64::from(tag.abstract_size_for_gas_metering()),
            contents.len() as u64,
        )?;
        Ok((tag, contents))
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
        let config = ProtocolConfig::get_for_max_version_UNSAFE();
        assert_eq!(config.forwarding_address_max_hops(), 3);
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

    /// Reroutes `credits` with no objects and returns the deposit events decoded.
    fn reroute_credits(
        registry: &dyn ImplicitSystemObjectResolver,
        credits: &mut [MoveAccumulatorEvent],
    ) -> Result<Vec<ForwardingDeposit>, RerouteError> {
        let config = config();
        let events = Resolver::new(&config, registry, &mut NoForwardingGas)
            .reroute(std::iter::empty(), credits)?;
        Ok(events
            .into_iter()
            .map(|(tag, contents)| {
                assert_eq!(tag, ForwardingDeposit::struct_tag(GAS::type_tag()));
                bcs::from_bytes(&contents).unwrap()
            })
            .collect())
    }

    fn assert_unresolvable<T: std::fmt::Debug>(result: Result<T, RerouteError>, needle: &str) {
        match result {
            Err(RerouteError::Unresolvable(message)) => {
                assert!(message.contains(needle), "{message}")
            }
            other => panic!("expected an unresolvable address containing {needle:?}: {other:?}"),
        }
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
        let mut credits = [credit(forwarding(3, 0), 500)];
        let deposits = reroute_credits(&registry, &mut credits).unwrap();
        assert_eq!(SuiAddress::from(credits[0].target_addr), ordinary(0xaa));
        let by_address: BTreeMap<SuiAddress, (SuiAddress, u64)> = deposits
            .into_iter()
            .map(|d| (d.forwarding_address, (d.master, d.amount)))
            .collect();
        assert_eq!(by_address.len(), 3);
        assert_eq!(by_address[&forwarding(3, 0)], (forwarding(2, 0), 500));
        assert_eq!(by_address[&forwarding(2, 0)], (forwarding(1, 0), 500));
        assert_eq!(by_address[&forwarding(1, 0)], (ordinary(0xaa), 500));
        assert_unresolvable(
            reroute_credits(&registry, &mut [credit(forwarding(4, 0), 1)]),
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
        let mut credits = [
            credit(forwarding(2, 0), 300),
            credit(forwarding(2, 0), 400),
            credit(forwarding(3, 0), 1000),
        ];
        let deposits = reroute_credits(&registry, &mut credits).unwrap();
        let by_address: BTreeMap<SuiAddress, (SuiAddress, u64)> = deposits
            .into_iter()
            .map(|d| (d.forwarding_address, (d.master, d.amount)))
            .collect();
        assert_eq!(by_address.len(), 3);
        assert_eq!(by_address[&forwarding(2, 0)], (forwarding(1, 0), 700));
        assert_eq!(by_address[&forwarding(3, 0)], (forwarding(1, 0), 1000));
        assert_eq!(by_address[&forwarding(1, 0)], (ordinary(0xaa), 1700));
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
            reroute_credits(&registry, &mut [credit(forwarding(9, 0), 1)]),
            "no registered master",
        );
        assert_unresolvable(
            reroute_credits(&registry, &mut [credit(forwarding(1, 0), 1)]),
            "no registered master (id 9)",
        );
        assert_unresolvable(
            reroute_credits(&registry, &mut [credit(forwarding(2, 0), 1)]),
            "variant 1",
        );
    }

    #[test]
    fn registry_errors_are_invariant_violations() {
        struct Broken;
        impl ImplicitSystemObjectResolver for Broken {
            fn object_available_balance(&self, _: SuiAddress, _: &TypeTag) -> SuiResult<u128> {
                unreachable!()
            }
            fn forwarding_master(&self, _: u64) -> SuiResult<Option<ForwardingMaster>> {
                Err(SuiErrorKind::ExecutionInvariantViolation.into())
            }
        }
        assert!(matches!(
            reroute_credits(&Broken, &mut [credit(forwarding(1, 0), 1)]),
            Err(RerouteError::InvariantViolation(_))
        ));
    }
}
