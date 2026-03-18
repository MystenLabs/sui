// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
pub mod checked {

    use crate::sui_types::gas::SuiGasStatusAPI;
    use crate::temporary_store::TemporaryStore;
    use either::Either;
    use indexmap::IndexMap;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::deny_list_v2::CONFIG_SETTING_DYNAMIC_FIELD_SIZE_FOR_GAS;
    use sui_types::digests::TransactionDigest;
    use sui_types::gas::{GasCostSummary, SuiGasStatus, deduct_gas};
    use sui_types::gas_model::gas_predicates::{
        charge_upgrades, dont_charge_budget_on_storage_oog,
    };
    use sui_types::{
        accumulator_event::AccumulatorEvent,
        base_types::{ObjectID, ObjectRef, SuiAddress},
        error::ExecutionError,
        gas_model::tables::GasStatus,
        is_system_package,
        object::Data,
    };
    use tracing::trace;

    /// Encapsulates the gas metering state (`SuiGasStatus`) and the payment source metadata,
    /// whether it is from a smashed list (coin objects or address-balance withdrawals) or
    /// un-metered. In other words, this serves the point of interaction between the on-chain data
    /// (coins and address balances) and the gas meter.
    #[derive(Debug)]
    pub struct GasCharger {
        tx_digest: TransactionDigest,
        gas_model_version: u64,
        payment: PaymentMetadata,
        gas_status: SuiGasStatus,
    }

    /// Internal representation of how a transaction's gas is being paid.
    /// `Unmetered` for for no payment (dev inspect and system transactions).
    /// `Smash` when one or more user-provided payment methods have been combined into a single
    /// source.
    #[derive(Debug)]
    enum PaymentMetadata {
        Unmetered,
        /// Contains the list of payments (coins and address balances) and additional metadata
        Smash(SmashMetadata),
    }

    /// State produced by smashing multiple gas payment sources into one.
    /// Tracks the combined balance (`total_smashed`), the target location where the
    /// smashed value lives, and the original payment methods for bookkeeping.
    /// Note that the target location (`gas_charge_location`) may differ from the first payment
    /// method in the list if it has ben overridden during execution.
    #[derive(Debug)]
    struct SmashMetadata {
        /// The location to charge gas from at the end of execution. Starts with the primary
        /// payment method but may be overridden.
        gas_charge_location: PaymentLocation,
        /// The total balance of all smashed payment methods.
        total_smashed: u64,
        /// The "primary" payment method that serves as the recipient of the `total_smashed`. Also,
        /// provides the initial location of the `gas_charge_location` before any overrides.
        smash_target: PaymentMethod,
        /// The original payment methods to be smashed into the `smash_target`. It does not include
        /// the `smash_target` itself. Keyed by location to guarantee uniqueness.
        smashed_payments: IndexMap<PaymentLocation, PaymentMethod>,
    }

    /// Public wrapper that describes how gas will be paid before smashing occurs.
    /// Constructed via `PaymentKind::unmetered()` or `PaymentKind::smash(methods)` and
    /// consumed by `GasCharger::new`.
    #[derive(Debug)]
    pub struct PaymentKind(PaymentKind_);

    /// Inner representation for `PaymentKind`. Kept private so construction is forced through
    /// the validation in `PaymentKind::smash`.
    #[derive(Debug)]
    enum PaymentKind_ {
        Unmetered,
        /// A non-empty map of gas coins or address balance withdrawals, keyed by location.
        /// The first entry is the smash target; all others are smashed into it.
        Smash(IndexMap<PaymentLocation, PaymentMethod>),
    }

    /// A single source of SUI used to pay for gas: either an coin object or a withdrawal
    /// reservation from an address balance.
    #[derive(Debug)]
    pub enum PaymentMethod {
        Coin(ObjectRef),
        AddressBalance(SuiAddress, /* withdrawal reservation */ u64),
    }

    /// Identifies where a gas payment lives, independent of its value (`ObjectRef` or reservation).
    /// Used often as a key, e.g. during smashing and during gas final charging.
    #[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
    pub enum PaymentLocation {
        Coin(ObjectID),
        AddressBalance(SuiAddress),
    }

    /// A resolved gas payment: the location that will receive the final charge or refund,
    /// paired with the total SUI available after smashing. Produced by
    /// `GasCharger::gas_payment_amount` and consumed by PTB execution to set up the
    /// runtime gas coin.
    #[derive(Debug, Clone, Copy)]
    pub struct GasPayment {
        /// The location of the gas payment (coin or address balance), which also serves as the
        /// target for smashed gas payments.
        pub location: PaymentLocation,
        /// The total amount available for gas payment after smashing
        pub amount: u64,
    }

    impl GasCharger {
        pub fn new(
            tx_digest: TransactionDigest,
            payment_kind: PaymentKind,
            gas_status: SuiGasStatus,
            temporary_store: &mut TemporaryStore<'_>,
            protocol_config: &ProtocolConfig,
        ) -> Self {
            let gas_model_version = protocol_config.gas_model_version();
            let payment = match payment_kind.0 {
                PaymentKind_::Unmetered => PaymentMetadata::Unmetered,
                PaymentKind_::Smash(mut payment_methods) => {
                    let (_, smash_target) = payment_methods.shift_remove_index(0).unwrap();
                    let mut metadata = SmashMetadata {
                        // dummy value set below in smash_gas
                        total_smashed: 0,
                        gas_charge_location: smash_target.location(),
                        smash_target,
                        smashed_payments: payment_methods,
                    };
                    metadata.smash_gas(&tx_digest, temporary_store);
                    PaymentMetadata::Smash(metadata)
                }
            };
            Self {
                tx_digest,
                gas_model_version,
                payment,
                gas_status,
            }
        }

        pub fn new_unmetered(tx_digest: TransactionDigest) -> Self {
            Self {
                tx_digest,
                gas_model_version: 6, // pick any of the latest, it should not matter
                payment: PaymentMetadata::Unmetered,
                gas_status: SuiGasStatus::new_unmetered(),
            }
        }

        // TODO: there is only one caller to this function that should not exist otherwise.
        //       Explore way to remove it.
        pub(crate) fn used_coins(&self) -> impl Iterator<Item = &'_ ObjectRef> {
            match &self.payment {
                PaymentMetadata::Unmetered => Either::Left(std::iter::empty()),
                PaymentMetadata::Smash(metadata) => Either::Right(metadata.used_coins()),
            }
        }

        // Override the gas payment location for smashing
        pub fn override_gas_charge_location(
            &mut self,
            location: PaymentLocation,
        ) -> Result<(), ExecutionError> {
            if let PaymentMetadata::Smash(metadata) = &mut self.payment {
                metadata.gas_charge_location = location;
                Ok(())
            } else {
                invariant_violation!("Can only override gas charge location in the smash-gas case")
            }
        }

        /// Return the amount available at the given input payment location.
        /// For unmetered, this is None.
        /// For smashed gas payments, this is the payment location and the total amount smashed.
        /// This information feels a bit brittle but should be used only by PTB execution.
        /// This might also differ from the final charge location, if override_gas_charge_location
        /// is used.
        pub fn gas_payment_amount(&self) -> Option<GasPayment> {
            match &self.payment {
                PaymentMetadata::Unmetered => None,
                PaymentMetadata::Smash(metadata) => Some(GasPayment {
                    location: metadata.smash_target.location(),
                    amount: metadata.total_smashed,
                }),
            }
        }

        pub fn gas_budget(&self) -> u64 {
            self.gas_status.gas_budget()
        }

        pub fn unmetered_storage_rebate(&self) -> u64 {
            self.gas_status.unmetered_storage_rebate()
        }

        pub fn no_charges(&self) -> bool {
            self.gas_status.gas_used() == 0
                && self.gas_status.storage_rebate() == 0
                && self.gas_status.storage_gas_units() == 0
        }

        pub fn is_unmetered(&self) -> bool {
            self.gas_status.is_unmetered()
        }

        pub fn move_gas_status(&self) -> &GasStatus {
            self.gas_status.move_gas_status()
        }

        pub fn move_gas_status_mut(&mut self) -> &mut GasStatus {
            self.gas_status.move_gas_status_mut()
        }

        pub fn into_gas_status(self) -> SuiGasStatus {
            self.gas_status
        }

        pub fn summary(&self) -> GasCostSummary {
            self.gas_status.summary()
        }

        // This function is called when the transaction is about to be executed.
        // It will smash all gas coins into a single one and set the logical gas coin
        // to be the first one in the list.
        // After this call, `gas_coin` will return it id of the gas coin.
        // This function panics if errors are found while operation on the gas coins.
        // Transaction and certificate input checks must have insured that all gas coins
        // are correct.
        fn smash_gas(&mut self, temporary_store: &mut TemporaryStore<'_>) {
            match &mut self.payment {
                // nothing to smash
                PaymentMetadata::Unmetered => (),
                PaymentMetadata::Smash(smash_metadata) => {
                    smash_metadata.smash_gas(&self.tx_digest, temporary_store);
                }
            }
        }

        //
        // Gas charging operations
        //

        pub fn track_storage_mutation(
            &mut self,
            object_id: ObjectID,
            new_size: usize,
            storage_rebate: u64,
        ) -> u64 {
            self.gas_status
                .track_storage_mutation(object_id, new_size, storage_rebate)
        }

        pub fn reset_storage_cost_and_rebate(&mut self) {
            self.gas_status.reset_storage_cost_and_rebate();
        }

        pub fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError> {
            self.gas_status.charge_publish_package(size)
        }

        pub fn charge_upgrade_package(&mut self, size: usize) -> Result<(), ExecutionError> {
            if charge_upgrades(self.gas_model_version) {
                self.gas_status.charge_publish_package(size)
            } else {
                Ok(())
            }
        }

        pub fn charge_input_objects(
            &mut self,
            temporary_store: &TemporaryStore<'_>,
        ) -> Result<(), ExecutionError> {
            let objects = temporary_store.objects();
            // TODO: Charge input object count.
            let _object_count = objects.len();
            // Charge bytes read
            let total_size = temporary_store
                .objects()
                .iter()
                // don't charge for loading Sui Framework or Move stdlib
                .filter(|(id, _)| !is_system_package(**id))
                .map(|(_, obj)| obj.object_size_for_gas_metering())
                .sum();
            self.gas_status.charge_storage_read(total_size)
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
            let cost_per_owner = bytes_read_per_owner * cost_per_byte;
            let owner_cost = cost_per_owner * (num_non_gas_coin_owners as usize);
            self.gas_status.charge_storage_read(owner_cost)
        }

        /// Resets any mutations, deletions, and events recorded in the store, as well as any storage costs and
        /// rebates, then Re-runs gas smashing. Effects on store are now as if we were about to begin execution
        pub fn reset(&mut self, temporary_store: &mut TemporaryStore<'_>) {
            temporary_store.drop_writes();
            self.gas_status.reset_storage_cost_and_rebate();
            self.smash_gas(temporary_store);
        }

        /// Entry point for gas charging.
        /// 1. Compute tx storage gas costs and tx storage rebates, update storage_rebate field of
        /// mutated objects
        /// 2. Deduct computation gas costs and storage costs, credit storage rebates.
        /// The happy path of this function follows (1) + (2) and is fairly simple.
        /// Most of the complexity is in the unhappy paths:
        /// - if execution aborted before calling this function, we have to dump all writes +
        ///   re-smash gas, then charge for storage
        /// - if we run out of gas while charging for storage, we have to dump all writes +
        ///   re-smash gas, then charge for storage again
        pub fn charge_gas<T>(
            &mut self,
            temporary_store: &mut TemporaryStore<'_>,
            execution_result: &mut Result<T, ExecutionError>,
        ) -> GasCostSummary {
            // at this point, we have done *all* charging for computation,
            // but have not yet set the storage rebate or storage gas units
            debug_assert!(self.gas_status.storage_rebate() == 0);
            debug_assert!(self.gas_status.storage_gas_units() == 0);

            if !matches!(&self.payment, PaymentMetadata::Unmetered) {
                // bucketize computation cost
                let is_move_abort = execution_result
                    .as_ref()
                    .err()
                    .map(|err| {
                        matches!(
                            err.kind(),
                            sui_types::execution_status::ExecutionErrorKind::MoveAbort(_, _)
                        )
                    })
                    .unwrap_or(false);
                // bucketize computation cost
                if let Err(err) = self.gas_status.bucketize_computation(Some(is_move_abort))
                    && execution_result.is_ok()
                {
                    *execution_result = Err(err);
                }

                // On error we need to dump writes, deletes, etc before charging storage gas
                if execution_result.is_err() {
                    self.reset(temporary_store);
                }
            }

            // compute and collect storage charges
            temporary_store.ensure_active_inputs_mutated();
            temporary_store.collect_storage_and_rebate(self);

            let gas_payment_location = match &self.payment {
                PaymentMetadata::Unmetered => {
                    // unmetered, nothing to charge
                    return GasCostSummary::default();
                }
                PaymentMetadata::Smash(metadata) => metadata.gas_charge_location,
            };
            if let PaymentLocation::Coin(_) = gas_payment_location {
                #[skip_checked_arithmetic]
                trace!(target: "replay_gas_info", "Gas smashing has occurred for this transaction");
            }

            if execution_result
                .as_ref()
                .err()
                .map(|err| {
                    matches!(
                        err.kind(),
                        sui_types::execution_status::ExecutionErrorKind::InsufficientFundsForWithdraw
                    )
                })
                .unwrap_or(false)
                && let PaymentLocation::AddressBalance(_) = gas_payment_location {
                    // If we don't have enough balance to withdraw, don't charge for gas
                    // TODO: consider charging gas if we have enough to reserve but not enough to cover all withdraws
                    return GasCostSummary::default();
            }

            self.compute_storage_and_rebate(temporary_store, execution_result);
            let cost_summary = self.gas_status.summary();
            let net_change = cost_summary.net_gas_usage();

            match gas_payment_location {
                PaymentLocation::AddressBalance(payer_address) => {
                    // TODO tracing?
                    if net_change != 0 {
                        let balance_type = sui_types::balance::Balance::type_tag(
                            sui_types::gas_coin::GAS::type_tag(),
                        );
                        let event = AccumulatorEvent::from_balance_change(
                            payer_address,
                            balance_type,
                            net_change.checked_neg().unwrap(),
                        )
                        .expect("Failed to create accumulator event for gas charging");
                        temporary_store.add_accumulator_event(event);
                    }
                }
                PaymentLocation::Coin(gas_object_id) => {
                    let mut gas_object =
                        temporary_store.read_object(&gas_object_id).unwrap().clone();
                    deduct_gas(&mut gas_object, net_change);
                    #[skip_checked_arithmetic]
                    trace!(net_change, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");
                    temporary_store.mutate_new_or_input_object(gas_object);
                }
            }
            cost_summary
        }

        /// Calculate total gas cost considering storage and rebate.
        ///
        /// First, we net computation, storage, and rebate to determine total gas to charge.
        ///
        /// If we exceed gas_budget, we set execution_result to InsufficientGas, failing the tx.
        /// If we have InsufficientGas, we determine how much gas to charge for the failed tx:
        ///
        /// v1: we set computation_cost = gas_budget, so we charge net (gas_budget - storage_rebates)
        /// v2: we charge (computation + storage costs for input objects - storage_rebates)
        ///     if the gas balance is still insufficient, we fall back to set computation_cost = gas_budget
        ///     so we charge net (gas_budget - storage_rebates)
        fn compute_storage_and_rebate<T>(
            &mut self,
            temporary_store: &mut TemporaryStore<'_>,
            execution_result: &mut Result<T, ExecutionError>,
        ) {
            if dont_charge_budget_on_storage_oog(self.gas_model_version) {
                self.handle_storage_and_rebate_v2(temporary_store, execution_result)
            } else {
                self.handle_storage_and_rebate_v1(temporary_store, execution_result)
            }
        }

        fn handle_storage_and_rebate_v1<T>(
            &mut self,
            temporary_store: &mut TemporaryStore<'_>,
            execution_result: &mut Result<T, ExecutionError>,
        ) {
            if let Err(err) = self.gas_status.charge_storage_and_rebate() {
                self.reset(temporary_store);
                self.gas_status.adjust_computation_on_out_of_gas();
                temporary_store.ensure_active_inputs_mutated();
                temporary_store.collect_rebate(self);
                if execution_result.is_ok() {
                    *execution_result = Err(err);
                }
            }
        }

        fn handle_storage_and_rebate_v2<T>(
            &mut self,
            temporary_store: &mut TemporaryStore<'_>,
            execution_result: &mut Result<T, ExecutionError>,
        ) {
            if let Err(err) = self.gas_status.charge_storage_and_rebate() {
                // we run out of gas charging storage, reset and try charging for storage again.
                // Input objects are touched and so they have a storage cost
                // Attempt to charge just for computation + input object storage costs - storage_rebate
                self.reset(temporary_store);
                temporary_store.ensure_active_inputs_mutated();
                temporary_store.collect_storage_and_rebate(self);
                if let Err(err) = self.gas_status.charge_storage_and_rebate() {
                    // we run out of gas attempting to charge for the input objects exclusively,
                    // deal with this edge case by not charging for storage: we charge (gas_budget - rebates).
                    self.reset(temporary_store);
                    self.gas_status.adjust_computation_on_out_of_gas();
                    temporary_store.ensure_active_inputs_mutated();
                    temporary_store.collect_rebate(self);
                    if execution_result.is_ok() {
                        *execution_result = Err(err);
                    }
                } else if execution_result.is_ok() {
                    *execution_result = Err(err);
                }
            }
        }
    }

    impl SmashMetadata {
        /// Iterates over all payment methods: the smash target followed by the smashed payments.
        fn payment_methods(&self) -> impl Iterator<Item = &'_ PaymentMethod> {
            std::iter::once(&self.smash_target).chain(self.smashed_payments.values())
        }

        fn smash_gas(
            &mut self,
            tx_digest: &TransactionDigest,
            temporary_store: &mut TemporaryStore<'_>,
        ) {
            // set gas charge location
            self.gas_charge_location = self.smash_target.location();

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
                        let balance_type = sui_types::balance::Balance::type_tag(
                            sui_types::gas_coin::GAS::type_tag(),
                        );
                        let event = AccumulatorEvent::from_balance_change(
                            *sui_address,
                            balance_type,
                            i64::try_from(*reservation).unwrap(),
                        )
                        .expect("Failed to create accumulator event for gas smashing");
                        temporary_store.add_accumulator_event(event);
                    }
                    PaymentMethod::Coin((id, _, _)) => {
                        temporary_store.delete_input_object(id);
                    }
                }
            }
            match &self.smash_target {
                PaymentMethod::AddressBalance(sui_address, reservation) => {
                    // The reservation here is only a maximal withdrawal from this address balance
                    // We do not need to withdraw here unless necessary, which will be done during
                    // gas charging
                    let deposit = total_smashed - *reservation;
                    if deposit != 0 {
                        let balance_type = sui_types::balance::Balance::type_tag(
                            sui_types::gas_coin::GAS::type_tag(),
                        );
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

        fn used_coins(&self) -> impl Iterator<Item = &'_ ObjectRef> {
            self.payment_methods().filter_map(|method| match method {
                PaymentMethod::Coin(obj_ref) => Some(obj_ref),
                PaymentMethod::AddressBalance(_, _) => None,
            })
        }
    }

    impl PaymentKind {
        pub fn unmetered() -> Self {
            Self(PaymentKind_::Unmetered)
        }

        pub fn smash(payment_methods: Vec<PaymentMethod>) -> Option<Self> {
            debug_assert!(
                !payment_methods.is_empty(),
                "GasCharger must have at least one payment method"
            );
            if payment_methods.is_empty() {
                return None;
            }
            let mut unique_methods = IndexMap::new();
            for payment_method in payment_methods {
                match (
                    unique_methods.entry(payment_method.location()),
                    payment_method,
                ) {
                    (indexmap::map::Entry::Vacant(entry), payment_method) => {
                        entry.insert(payment_method);
                    }
                    (
                        indexmap::map::Entry::Occupied(mut occupied),
                        PaymentMethod::AddressBalance(other, additional),
                    ) => {
                        let PaymentMethod::AddressBalance(addr, amount) = occupied.get_mut() else {
                            unreachable!("Payment method does not match location")
                        };
                        assert_eq!(*addr, other, "Payment method does not match location");
                        *amount += additional;
                    }
                    (indexmap::map::Entry::Occupied(_), _) => {
                        debug_assert!(
                            false,
                            "Duplicate coin payment method found, \
                             which should have been prevented by input checks"
                        );
                        return None;
                    }
                }
            }
            Some(Self(PaymentKind_::Smash(unique_methods)))
        }
    }

    impl PaymentMethod {
        pub fn location(&self) -> PaymentLocation {
            match self {
                PaymentMethod::Coin(obj_ref) => PaymentLocation::Coin(obj_ref.0),
                PaymentMethod::AddressBalance(addr, _) => PaymentLocation::AddressBalance(*addr),
            }
        }
    }
}
