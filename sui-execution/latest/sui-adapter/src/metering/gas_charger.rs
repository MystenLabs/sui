// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metering::gas_state::{
    ExecutionMeter, GasState, Metering, StorageCharge, StorageRates, StorageTier,
};
use crate::temporary_store::TemporaryStore;
use indexmap::IndexMap;
use mysten_common::assert_reachable;
use sui_protocol_config::ProtocolConfig;
use sui_types::coin_reservation::ParsedDigest;
use sui_types::deny_list_v2::CONFIG_SETTING_DYNAMIC_FIELD_SIZE_FOR_GAS;
use sui_types::digests::TransactionDigest;
use sui_types::error::ExecutionErrorTrait;
use sui_types::gas::{GasCostSummary, GasUsageReport, deduct_gas, execution_gas_params};
use sui_types::transaction::{GasData, TransactionKind, is_gasless_transaction};
use sui_types::{
    accumulator_event::AccumulatorEvent,
    base_types::{ObjectID, ObjectRef, SuiAddress},
    error::ExecutionError,
    gas_model::tables::GasStatus,
    object::Data,
};
use tracing::trace;

// The gas state and the payment of a transaction. The engine settles each phase here, and
// `charge` applies the final charge to the payment.
#[derive(Debug)]
pub(crate) struct GasCharger {
    tx_digest: TransactionDigest,
    payment: PaymentMetadata,
    state: GasState,
}

// Internal representation of how a transaction's gas is being paid.
// `Unmetered` for no payment (dev inspect and system transactions).
// `Gasless` for metered-but-free transactions (gas is metered but not charged).
// `Unsmashed` until `smash_gas` combines the signed payment methods.
// `Smash` when one or more user-provided payment methods have been combined into a single
// source.
#[derive(Debug)]
enum PaymentMetadata {
    Unmetered,
    Gasless,
    // Never empty.
    Unsmashed(Vec<PaymentMethod>),
    // Contains the list of payments (coins and address balances) and additional metadata
    Smash(SmashMetadata),
}

// State produced by smashing the gas payment sources into one. The charge goes to the smash
// target unless the PTB moved the gas coin.
#[derive(Debug)]
struct SmashMetadata {
    // Where the PTB moved the gas coin, if it did.
    moved_to: Option<PaymentLocation>,
    // The total balance of all smashed payment methods.
    total_smashed: u64,
    // The primary payment method, which receives `total_smashed`.
    smash_target: PaymentMethod,
    // The original payment methods to be smashed into the `smash_target`. It does not include
    // the `smash_target` itself. Keyed by location to guarantee uniqueness.
    smashed_payments: IndexMap<PaymentLocation, PaymentMethod>,
}

// A single source of SUI used to pay for gas: either a coin object or a withdrawal
// reservation from an address balance.
#[derive(Debug, Clone, Copy)]
enum PaymentMethod {
    Coin(ObjectRef),
    AddressBalance(SuiAddress, /* withdrawal reservation */ u64),
}

// Identifies where a gas payment lives, independent of its value (`ObjectRef` or reservation).
// Used often as a key, e.g. during smashing and during gas final charging.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub(crate) enum PaymentLocation {
    Coin(ObjectID),
    AddressBalance(SuiAddress),
}

// A resolved gas payment: the location that will receive the final charge or refund,
// paired with the total SUI available after smashing. Produced by
// `GasMeter::payment` and consumed by PTB execution to set up the
// runtime gas coin.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GasPayment {
    // The location of the gas payment (coin or address balance), which also serves as the
    // target for smashed gas payments.
    pub location: PaymentLocation,
    // The total amount available for gas payment after smashing
    pub amount: u64,
}

impl GasCharger {
    // Budget and price follow the rule signing uses. The store is untouched until
    // `smash_gas`.
    pub fn new(
        tx_digest: TransactionDigest,
        gas_data: &GasData,
        transaction_kind: &TransactionKind,
        reference_gas_price: u64,
        protocol_config: &ProtocolConfig,
    ) -> Self {
        let metering = match execution_gas_params(
            gas_data,
            transaction_kind,
            reference_gas_price,
            protocol_config,
        ) {
            None => Metering::Unmetered,
            Some((budget, price)) => Metering::Metered {
                budget,
                price,
                reference_gas_price,
            },
        };
        let payment = match metering {
            Metering::Unmetered => PaymentMetadata::Unmetered,
            Metering::Metered { .. } if is_gasless_transaction(gas_data, transaction_kind) => {
                PaymentMetadata::Gasless
            }
            Metering::Metered { .. } => {
                PaymentMetadata::Unsmashed(PaymentMethod::from_gas_data(gas_data))
            }
        };
        Self {
            tx_digest,
            payment,
            state: GasState::new(metering, protocol_config),
        }
    }

    // Terms and payment.

    pub fn is_unmetered(&self) -> bool {
        self.state.is_unmetered()
    }

    pub fn is_gasless(&self) -> bool {
        matches!(self.payment, PaymentMetadata::Gasless)
    }

    // The price the transaction is metered at, zero when unmetered.
    pub fn gas_price(&self) -> u64 {
        self.state.price()
    }

    // The reference gas price of the epoch, zero when unmetered.
    pub fn reference_gas_price(&self) -> u64 {
        self.state.reference_gas_price()
    }

    // The smash target and the total smashed. `None` for unmetered and gasless.
    fn gas_payment(&self) -> Option<GasPayment> {
        match &self.payment {
            PaymentMetadata::Unmetered | PaymentMetadata::Gasless => None,
            PaymentMetadata::Unsmashed(_) => unreachable!("gas is smashed before execution"),
            PaymentMetadata::Smash(metadata) => Some(GasPayment {
                location: metadata.smash_target.location(),
                amount: metadata.total_smashed,
            }),
        }
    }

    // The gas usage so far, for the exits that skip `charge`.
    pub fn gas_usage_report(&self) -> GasUsageReport {
        self.state.gas_usage_report()
    }

    // Payment setup.

    // Smash the signed payments into the first one. Input checks validate the gas coins, so
    // a bad coin panics.
    pub fn smash_gas(
        &mut self,
        temporary_store: &mut TemporaryStore<'_>,
    ) -> Result<(), ExecutionError> {
        let PaymentMetadata::Unsmashed(methods) = &self.payment else {
            debug_assert!(
                !matches!(self.payment, PaymentMetadata::Smash(_)),
                "gas is smashed once"
            );
            return Ok(());
        };
        let mut metadata = SmashMetadata::new(methods)?;
        metadata.smash_gas(&self.tx_digest, temporary_store);
        self.payment = PaymentMetadata::Smash(metadata);
        Ok(())
    }

    // Smash again after the writes were dropped. The charge goes back to the smash target.
    pub fn resmash(&mut self, temporary_store: &mut TemporaryStore<'_>) {
        if let PaymentMetadata::Smash(metadata) = &mut self.payment {
            metadata.reset_charge_location();
            metadata.smash_gas(&self.tx_digest, temporary_store);
        }
    }

    // Phases, in execution order.

    pub fn settle_pre_execution(&mut self, mist: u64) -> Result<(), ExecutionError> {
        self.state.settle_pre_execution(mist)
    }

    // Open the execution phase. `settle_execution` closes it.
    pub fn gas_meter(&mut self) -> Result<GasMeter, ExecutionError> {
        Ok(GasMeter {
            execution_meter: self.state.execution_meter()?,
            payment: self.gas_payment(),
            budget: self.state.budget(),
            unmetered: self.state.is_unmetered(),
            charge_location: None,
        })
    }

    // An execution error wins over running out of gas.
    pub fn settle_execution<T, E: ExecutionErrorTrait>(
        &mut self,
        gas_meter: GasMeter,
        result: Result<T, E>,
    ) -> Result<T, E> {
        let GasMeter {
            execution_meter,
            charge_location,
            ..
        } = gas_meter;
        if let (Some(location), PaymentMetadata::Smash(metadata)) =
            (charge_location, &mut self.payment)
        {
            metadata.move_charge_location(location);
        }
        let is_move_abort = matches!(
            result.as_ref().err().map(|e| e.kind()),
            Some(sui_types::execution_status::ExecutionErrorKind::MoveAbort(
                ..
            ))
        );
        let charge = self.state.settle_execution(execution_meter, is_move_abort);
        match result {
            Ok(v) => charge.map(|_| v).map_err(Into::into),
            Err(e) => Err(e),
        }
    }

    pub fn storage_rates(&self) -> StorageRates {
        self.state.storage_rates()
    }

    pub fn settle_storage(
        &mut self,
        charge: StorageCharge,
        tier: StorageTier,
    ) -> Result<(), ExecutionError> {
        self.state.settle_storage(charge, tier)
    }

    // Payment.

    // Apply the final charge to the payment.
    pub fn charge<T, E: ExecutionErrorTrait>(
        self,
        temporary_store: &mut TemporaryStore<'_>,
        execution_result: &Result<T, E>,
    ) -> ChargedGas {
        let cost_summary = match &self.payment {
            PaymentMetadata::Unmetered => {
                // Park unmetered (system-tx) storage rebate into 0x5 so SUI is not dropped.
                let unmetered_storage_rebate = self.state.unmetered_storage_rebate();
                temporary_store.conserve_unmetered_storage_rebate(unmetered_storage_rebate);
                GasCostSummary::default()
            }
            PaymentMetadata::Unsmashed(_) => {
                unreachable!("gas is smashed before it is charged")
            }
            PaymentMetadata::Gasless => {
                if execution_result.is_err() {
                    GasCostSummary::default()
                } else {
                    let cost_summary = self.state.summary();
                    let storage_cost = cost_summary.storage_cost;
                    assert!(
                        storage_cost == 0,
                        "Gasless transaction must not incur storage cost, got {storage_cost}"
                    );
                    let sender_rebate = cost_summary.storage_rebate;
                    GasCostSummary {
                        computation_cost: sender_rebate,
                        storage_cost: 0,
                        storage_rebate: sender_rebate,
                        non_refundable_storage_fee: cost_summary.non_refundable_storage_fee,
                    }
                }
            }
            PaymentMetadata::Smash(metadata) => {
                if let PaymentLocation::Coin(_) = metadata.charge_location() {
                    trace!(target: "replay_gas_info", "Gas smashing has occurred for this transaction");
                }
                let cost_summary = self.state.summary();
                metadata.apply(temporary_store, cost_summary)
            }
        };
        let (used_coins, charge_location) = match &self.payment {
            PaymentMetadata::Smash(metadata) => (
                metadata.used_coins().copied().collect(),
                Some(metadata.charge_location()),
            ),
            _ => (vec![], None),
        };
        ChargedGas {
            cost_summary,
            gas_report: self.state.gas_usage_report(),
            gas_coin: self
                .gas_payment()
                .and_then(|payment| match payment.location {
                    PaymentLocation::Coin(coin_id) => Some(coin_id),
                    PaymentLocation::AddressBalance(_) => None,
                }),
            used_coins,
            charge_location,
        }
    }
}

// What the invariant checks and the effects need from the gas after the final charge.
#[derive(Debug)]
pub(crate) struct ChargedGas {
    pub cost_summary: GasCostSummary,
    pub gas_report: GasUsageReport,
    // The smash target when it is a coin. The effects name it as the gas object.
    pub gas_coin: Option<ObjectID>,
    // Every coin the payment smashed, the smash target included.
    pub used_coins: Vec<ObjectRef>,
    // Where the charge went. `None` without a payment (unmetered and gasless).
    pub charge_location: Option<PaymentLocation>,
}

// The meter the PTB charges into during the execution phase, with the payment it may spend
// from.
#[derive(Debug)]
pub(crate) struct GasMeter {
    execution_meter: ExecutionMeter,
    payment: Option<GasPayment>,
    budget: u64,
    unmetered: bool,
    charge_location: Option<PaymentLocation>,
}

impl GasMeter {
    // The meter for genesis: nothing is metered and there is no payment.
    pub fn unmetered(protocol_config: &ProtocolConfig) -> Self {
        Self {
            execution_meter: ExecutionMeter::unmetered(protocol_config),
            payment: None,
            budget: 0,
            unmetered: true,
            charge_location: None,
        }
    }

    // Move meter.

    pub fn move_gas_status(&self) -> &GasStatus {
        self.execution_meter.status()
    }

    pub fn move_gas_status_mut(&mut self) -> &mut GasStatus {
        self.execution_meter.status_mut()
    }

    // Terms and payment.

    pub fn budget(&self) -> u64 {
        self.budget
    }

    pub fn is_unmetered(&self) -> bool {
        self.unmetered
    }

    // The smash target and the total smashed. `None` for unmetered and gasless.
    pub fn payment(&self) -> Option<GasPayment> {
        self.payment
    }

    // Record where the PTB moved the gas coin.
    pub fn set_charge_location(&mut self, location: PaymentLocation) -> Result<(), ExecutionError> {
        if self.payment.is_none() {
            invariant_violation!("Can only override gas charge location in the smash-gas case")
        }
        self.charge_location = Some(location);
        Ok(())
    }

    // Byte charges.

    pub fn charge_input_object(
        &mut self,
        size: usize,
        is_package: bool,
    ) -> Result<(), ExecutionError> {
        self.execution_meter.charge_input_object(size, is_package)
    }

    pub fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError> {
        self.execution_meter.charge_publish_package(size)
    }

    pub fn charge_coin_transfers(
        &mut self,
        protocol_config: &ProtocolConfig,
        num_non_gas_coin_owners: u64,
    ) -> Result<(), ExecutionError> {
        // times two for the global pause and per-address settings
        // this "overcharges" slightly since it does not check the global pause for each owner
        // but rather each coin type.
        let bytes_read_per_owner = CONFIG_SETTING_DYNAMIC_FIELD_SIZE_FOR_GAS;
        // associate the cost with dynamic field access so that it will increase if/when this
        // cost increases
        let cost_per_byte =
            protocol_config.dynamic_field_borrow_child_object_type_cost_per_byte() as usize;
        let owner_cost = bytes_read_per_owner
            .saturating_mul(cost_per_byte)
            .saturating_mul(num_non_gas_coin_owners as usize);
        self.execution_meter.charge_read_bytes(owner_cost)
    }
}

impl SmashMetadata {
    // The first payment is the smash target. Reservations on one address are summed. A
    // duplicate gas coin or an overflowing sum is an invariant violation.
    fn new(methods: &[PaymentMethod]) -> Result<Self, ExecutionError> {
        let invalid = || {
            ExecutionError::invariant_violation(
                "invalid gas payment: duplicate gas coin or reservation overflow",
            )
        };
        let mut unique_methods: IndexMap<PaymentLocation, PaymentMethod> = IndexMap::new();
        for &method in methods {
            match (unique_methods.entry(method.location()), method) {
                (indexmap::map::Entry::Vacant(entry), method) => {
                    entry.insert(method);
                }
                (
                    indexmap::map::Entry::Occupied(mut occupied),
                    PaymentMethod::AddressBalance(other, additional),
                ) => {
                    let PaymentMethod::AddressBalance(addr, amount) = occupied.get_mut() else {
                        unreachable!("Payment method does not match location")
                    };
                    assert_eq!(*addr, other, "Payment method does not match location");
                    *amount = amount.checked_add(additional).ok_or_else(invalid)?;
                }
                (indexmap::map::Entry::Occupied(_), _) => return Err(invalid()),
            }
        }
        let (_, smash_target) = unique_methods.shift_remove_index(0).ok_or_else(invalid)?;
        Ok(Self {
            moved_to: None,
            // set by smash_gas
            total_smashed: 0,
            smash_target,
            smashed_payments: unique_methods,
        })
    }

    // The charge location.

    fn charge_location(&self) -> PaymentLocation {
        self.moved_to.unwrap_or(self.smash_target.location())
    }

    fn move_charge_location(&mut self, location: PaymentLocation) {
        self.moved_to = Some(location);
    }

    fn reset_charge_location(&mut self) {
        self.moved_to = None;
    }

    // Coin and balance operations.

    // Iterates over all payment methods: the smash target followed by the smashed payments.
    fn payment_methods(&self) -> impl Iterator<Item = &'_ PaymentMethod> {
        std::iter::once(&self.smash_target).chain(self.smashed_payments.values())
    }

    fn smash_gas(
        &mut self,
        tx_digest: &TransactionDigest,
        temporary_store: &mut TemporaryStore<'_>,
    ) {
        // sum the value of all gas coins
        let total_smashed = self
            .payment_methods()
            .map(|payment| match payment {
                PaymentMethod::AddressBalance(_, reservation) => Ok(*reservation),
                PaymentMethod::Coin(obj_ref) => {
                    let obj_data = temporary_store
                        .objects()
                        .get(&obj_ref.0)
                        .map(|obj| &obj.data);
                    let Some(Data::Move(move_obj)) = obj_data else {
                        return Err(ExecutionError::invariant_violation(
                            "Provided non-gas coin object as input for gas!",
                        ));
                    };
                    if !move_obj.type_().is_gas_coin() {
                        return Err(ExecutionError::invariant_violation(
                            "Provided non-gas coin object as input for gas!",
                        ));
                    }
                    Ok(move_obj.get_coin_value_unsafe())
                }
            })
            .collect::<Result<Vec<u64>, ExecutionError>>()
            // transaction and certificate input checks must have insured that all gas coins
            // are valid
            .unwrap_or_else(|_| {
                panic!(
                    "Unable to process gas payments for transaction {}",
                    tx_digest
                )
            })
            .iter()
            .sum();
        // If it is 0, then we are smashing for the first time (at the beginning of execution).
        // If it is non-zero, then we are re-smashing after a reset (due to some sort of
        // failure in charging for gas), and the total should not change.
        debug_assert!(
            self.total_smashed == 0 || self.total_smashed == total_smashed,
            "Gas smashing should not change after a reset"
        );
        self.total_smashed = total_smashed;

        let smash_location = self.smash_target.location();
        // delete all gas objects except the smash target
        for payment_method in self.smashed_payments.values() {
            let location = payment_method.location();
            assert_ne!(location, smash_location, "Payment methods must be unique");
            match payment_method {
                PaymentMethod::AddressBalance(sui_address, reservation) => {
                    assert_reachable!("smashed payment is address-balance reservation");
                    let balance_type =
                        sui_types::balance::Balance::type_tag(sui_types::gas_coin::GAS::type_tag());
                    let event = AccumulatorEvent::from_balance_change(
                        *sui_address,
                        balance_type,
                        i64::try_from(*reservation).unwrap().checked_neg().unwrap(),
                    )
                    .expect("Failed to create accumulator event for gas smashing");
                    temporary_store.add_accumulator_event(event);
                }
                PaymentMethod::Coin((id, _, _)) => {
                    assert_reachable!("smashed payment is coin object");
                    temporary_store.delete_input_object(id);
                }
            }
        }
        match &self.smash_target {
            PaymentMethod::AddressBalance(sui_address, reservation) => {
                assert_reachable!("smash target is address-balance reservation");
                // The reservation here is only a maximal withdrawal from this address balance
                // We do not need to withdraw here unless necessary, which will be done during
                // gas charging
                let deposit = total_smashed
                    .checked_sub(*reservation)
                    .expect("the total smashed includes the target's reservation");
                if deposit != 0 {
                    let balance_type =
                        sui_types::balance::Balance::type_tag(sui_types::gas_coin::GAS::type_tag());
                    let event = AccumulatorEvent::from_balance_change(
                        *sui_address,
                        balance_type,
                        i64::try_from(deposit).unwrap(),
                    )
                    .expect("Failed to create accumulator event for gas smashing");
                    temporary_store.add_accumulator_event(event);
                }
            }
            PaymentMethod::Coin((gas_coin_id, _, _)) => {
                let mut primary_gas_object = temporary_store
                    .objects()
                    .get(gas_coin_id)
                    // unwrap should be safe because we checked that this exists in `self.objects()` above
                    .unwrap_or_else(|| {
                        panic!(
                            "Invariant violation: gas coin not found in store in txn {}",
                            tx_digest
                        )
                    })
                    .clone();
                primary_gas_object
                    .data
                    .try_as_move_mut()
                    // unwrap should be safe because we checked that the primary gas object was a coin object above.
                    .unwrap_or_else(|| {
                        panic!(
                            "Invariant violation: invalid coin object in txn {}",
                            tx_digest
                        )
                    })
                    .set_coin_value_unsafe(total_smashed);
                temporary_store.mutate_input_object(primary_gas_object);
            }
        }
    }

    // Apply the net charge or refund to the gas coin, or to the address balance through an
    // accumulator event.
    fn apply(
        &self,
        temporary_store: &mut TemporaryStore<'_>,
        cost_summary: GasCostSummary,
    ) -> GasCostSummary {
        let net_change = cost_summary.net_gas_usage();
        match self.charge_location() {
            PaymentLocation::AddressBalance(payer_address) => {
                if net_change != 0 {
                    let balance_type =
                        sui_types::balance::Balance::type_tag(sui_types::gas_coin::GAS::type_tag());
                    let event = AccumulatorEvent::from_balance_change(
                        payer_address,
                        balance_type,
                        net_change
                            .checked_neg()
                            .expect("net gas usage is never i64::MIN"),
                    )
                    .expect("Failed to create accumulator event for gas charging");
                    temporary_store.add_accumulator_event(event);
                }
            }
            PaymentLocation::Coin(gas_object_id) => {
                let mut gas_object = temporary_store
                    .read_object(&gas_object_id)
                    .expect("gas coin is an input object and present after smashing")
                    .clone();
                deduct_gas(&mut gas_object, net_change);
                trace!(net_change, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");
                temporary_store.mutate_new_or_input_object(gas_object);
            }
        }
        cost_summary
    }

    fn used_coins(&self) -> impl Iterator<Item = &'_ ObjectRef> {
        self.payment_methods().filter_map(|method| match method {
            PaymentMethod::Coin(obj_ref) => Some(obj_ref),
            PaymentMethod::AddressBalance(_, _) => None,
        })
    }
}

impl PaymentMethod {
    // The payments as signed: each gas object is a coin, or an address balance reservation
    // when its digest encodes one. With no gas objects the budget is reserved on the owner.
    fn from_gas_data(gas_data: &GasData) -> Vec<Self> {
        if gas_data.payment.is_empty() {
            return vec![PaymentMethod::AddressBalance(
                gas_data.owner,
                gas_data.budget,
            )];
        }
        gas_data
            .payment
            .iter()
            .map(|entry| match ParsedDigest::try_from(entry.2) {
                Ok(parsed) => {
                    PaymentMethod::AddressBalance(gas_data.owner, parsed.reservation_amount())
                }
                Err(_) => PaymentMethod::Coin(*entry),
            })
            .collect()
    }

    fn location(&self) -> PaymentLocation {
        match self {
            PaymentMethod::Coin(obj_ref) => PaymentLocation::Coin(obj_ref.0),
            PaymentMethod::AddressBalance(addr, _) => PaymentLocation::AddressBalance(*addr),
        }
    }
}
