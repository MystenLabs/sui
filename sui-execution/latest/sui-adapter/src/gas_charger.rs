// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
pub mod checked {

    use crate::sui_types::gas::SuiGasStatusAPI;
    use crate::temporary_store::TemporaryStore;
    use either::Either;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::deny_list_v2::CONFIG_SETTING_DYNAMIC_FIELD_SIZE_FOR_GAS;
    use sui_types::gas::{GasCostSummary, SuiGasStatus, deduct_gas};
    use sui_types::gas_model::gas_predicates::{
        charge_upgrades, dont_charge_budget_on_storage_oog,
    };
    use sui_types::{
        accumulator_event::AccumulatorEvent,
        base_types::{ObjectID, ObjectRef, SuiAddress},
        digests::TransactionDigest,
        error::ExecutionError,
        gas_model::tables::GasStatus,
        is_system_package,
        object::Data,
    };
    use tracing::trace;

    /// Tracks all gas operations for a single transaction.
    /// This is the main entry point for gas accounting.
    /// All the information about gas is stored in this object.
    /// The objective here is two-fold:
    /// 1- Isolate al version info into a single entry point. This file and the other gas
    ///    related files are the only one that check for gas version.
    /// 2- Isolate all gas accounting into a single implementation. Gas objects are not
    ///    passed around, and they are retrieved from this instance.
    #[derive(Debug)]
    pub struct GasCharger {
        tx_digest: TransactionDigest,
        gas_model_version: u64,
        /// Empty if unmetered
        payment_methods: Vec<PaymentMethod>,
        //smashed_gas_coin_bud
        gas_status: SuiGasStatus,
    }

    #[derive(Debug)]
    pub enum PaymentMethod {
        Coin(ObjectRef),
        AddressBalance(SuiAddress, /* withdrawal reservation */ u64),
    }

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub enum PaymentLocation {
        Coin(ObjectID),
        AddressBalance(SuiAddress),
    }

    impl PaymentMethod {
        pub fn location(&self) -> PaymentLocation {
            match self {
                PaymentMethod::Coin(obj_ref) => PaymentLocation::Coin(obj_ref.0),
                PaymentMethod::AddressBalance(addr, _) => PaymentLocation::AddressBalance(*addr),
            }
        }
    }

    impl GasCharger {
        pub fn new(
            tx_digest: TransactionDigest,
            payment_methods: Vec<PaymentMethod>,
            gas_status: SuiGasStatus,
            protocol_config: &ProtocolConfig,
        ) -> Self {
            assert!(
                !payment_methods.is_empty(),
                "GasCharger must have at least one payment method"
            );
            let gas_model_version = protocol_config.gas_model_version();
            Self {
                tx_digest,
                gas_model_version,
                payment_methods,
                gas_status,
            }
        }

        pub fn new_unmetered(tx_digest: TransactionDigest) -> Self {
            Self {
                tx_digest,
                gas_model_version: 6, // pick any of the latest, it should not matter
                payment_methods: vec![],
                gas_status: SuiGasStatus::new_unmetered(),
            }
        }

        // // TODO: there is only one caller to this function that should not exist otherwise.
        // //       Explore way to remove it.
        // pub(crate) fn gas_coins(&self) -> impl Iterator<Item = &'_ ObjectRef> {
        //     match &self.payment_method {
        //         PaymentMethod::Coins(gas_coins) => Either::Left(gas_coins.iter()),
        //         PaymentMethod::AddressBalance(_) | PaymentMethod::Unmetered => {
        //             Either::Right(std::iter::empty())
        //         }
        //     }
        // }
        //

        fn smash_target(&self) -> Option<&PaymentMethod> {
            self.payment_methods.first()
        }

        // Return the payment location for this transaction, or None if it was unmetered.
        pub fn gas_payment_location(&self) -> Option<PaymentLocation> {
            self.payment_methods.first().map(|method| method.location())
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
        pub fn smash_gas(&mut self, temporary_store: &mut TemporaryStore<'_>) {
            let payment_count = self.payment_methods.len();
            let Some(primary_method) = self.smash_target() else {
                // unmetered, nothing to smash
                return;
            };
            let primary_location = primary_method.location();
            if self.payment_methods.len() == 1 {
                // only one payment method, nothing to smash
                return;
            }

            // sum the value of all gas coins
            let total = self
                .payment_methods
                .iter()
                .map(|payment| match payment {
                    PaymentMethod::AddressBalance(_, reservation) => Ok(*reservation),
                    PaymentMethod::Coin(obj_ref) => {
                        let obj = temporary_store.objects().get(&obj_ref.0).unwrap();
                        let Data::Move(move_obj) = &obj.data else {
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
                        self.tx_digest
                    )
                })
                .iter()
                .sum();
            // delete all gas objects except the primary_gas_object
            for payment_method in &self.payment_methods[1..] {
                let location = payment_method.location();
                assert_ne!(location, primary_location, "Payment methods must be unique");
                match payment_method {
                    PaymentMethod::AddressBalance(sui_address, reservation) => {
                        temporary_store.withdraw_from_address_balance(*sui_address, *reservation);
                    }
                    PaymentMethod::Coin((id, _, _)) => {
                        temporary_store.delete_input_object(id);
                    }
                }
            }
            match primary_method {
                PaymentMethod::AddressBalance(sui_address, reservation) => {
                    temporary_store.withdraw_from_address_balance(*sui_address, *reservation);
                    temporary_store.deposit_to_address_balance(*sui_address, total);
                }
                PaymentMethod::Coin((gas_coin_id, _, _)) => {
                    let mut primary_gas_object = temporary_store
                        .objects()
                        .get(&gas_coin_id)
                        // unwrap should be safe because we checked that this exists in `self.objects()` above
                        .unwrap_or_else(|| {
                            panic!(
                                "Invariant violation: gas coin not found in store in txn {}",
                                self.tx_digest
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
                                self.tx_digest
                            )
                        })
                        .set_coin_value_unsafe(total);
                    temporary_store.mutate_input_object(primary_gas_object);
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

            if !self.payment_methods.is_empty() {
                // bucketize computation cost
                let is_move_abort = execution_result
                    .as_ref()
                    .err()
                    .map(|err| {
                        matches!(
                            err.kind(),
                            sui_types::execution_status::ExecutionFailureStatus::MoveAbort(_, _)
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

            let Some(primary_method) = self.smash_target() else {
                // unmetered, nothing to charge
                return GasCostSummary::default();
            };

            if let PaymentMethod::Coin(_) = primary_method {
                #[skip_checked_arithmetic]
                trace!(target: "replay_gas_info", "Gas smashing has occurred for this transaction");
            }

            if execution_result
                .as_ref()
                .err()
                .map(|err| {
                    matches!(
                        err.kind(),
                        sui_types::execution_status::ExecutionFailureStatus::InsufficientFundsForWithdraw
                    )
                })
                .unwrap_or(false)
                && let PaymentMethod::AddressBalance(_,_) = primary_method {
                    // If we don't have enough balance to withdraw, don't charge for gas
                    // TODO: consider charging gas if we have enough to reserve but not enough to cover all withdraws
                    return GasCostSummary::default();
            }

            self.compute_storage_and_rebate(temporary_store, execution_result);
            let cost_summary = self.gas_status.summary();
            let net_change = cost_summary.net_gas_usage();

            match primary_method {
                PaymentMethod::AddressBalance(payer_address, _) => {
                    // if net_change != 0 {
                    //     let balance_type = sui_types::balance::Balance::type_tag(
                    //         sui_types::gas_coin::GAS::type_tag(),
                    //     );
                    //     let accumulator_event = AccumulatorEvent::from_balance_change(
                    //         payer_address,
                    //         balance_type,
                    //         net_change,
                    //     )
                    //     .expect("Failed to create accumulator event for gas balance");

                    //     temporary_store.add_accumulator_event(accumulator_event);
                    // }
                    // TODO tracing?
                    if net_change < 0 {
                        temporary_store
                            .withdraw_from_address_balance(*payer_address, -net_change as u64);
                    } else if net_change > 0 {
                        temporary_store
                            .deposit_to_address_balance(*payer_address, net_change as u64);
                    }
                }
                PaymentMethod::Coin((gas_object_id, _, _)) => {
                    let mut gas_object =
                        temporary_store.read_object(&gas_object_id).unwrap().clone();
                    deduct_gas(&mut gas_object, net_change);
                    #[skip_checked_arithmetic]
                    trace!(net_change, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");

                    temporary_store.mutate_input_object(gas_object);
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
}
