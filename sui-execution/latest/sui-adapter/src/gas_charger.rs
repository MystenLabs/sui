// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
pub mod checked {

    use crate::sui_types::gas::SuiGasStatusAPI;
    use crate::temporary_store::TemporaryStore;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::{SequenceNumber, TxContext};
    use sui_types::deny_list_v2::CONFIG_SETTING_DYNAMIC_FIELD_SIZE_FOR_GAS;
    use sui_types::gas::{GasCostSummary, SuiGasStatus, deduct_gas, get_gas_balance};
    use sui_types::gas_model::gas_predicates::{
        charge_upgrades, dont_charge_budget_on_storage_oog,
    };
    use sui_types::object::{MoveObject, Object, Owner};
    use sui_types::{
        base_types::{ObjectID, ObjectRef, SuiAddress},
        digests::TransactionDigest,
        error::ExecutionError,
        gas_model::tables::GasStatus,
        is_system_package,
    };

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
        payment_method: PaymentMethod,
        // this is the first gas coin in `gas_coins` and the one that all others will
        // be smashed into. It can be None for system transactions when `gas_coins` is empty.
        smashed_gas_coin: Option<ObjectID>,
        //smashed_gas_coin_bud
        gas_status: SuiGasStatus,
    }

    #[derive(Debug)]
    pub enum PaymentMethod {
        Unmetered,
        Metered {
            primary_coin: Option<ObjectRef>,
            additional_coins: Vec<ObjectRef>,
            address_balance_payer: SuiAddress,
            available_address_balance_gas: u64,
        },
    }

    impl PaymentMethod {
        pub fn is_unmetered(&self) -> bool {
            matches!(self, PaymentMethod::Unmetered)
        }
    }

    impl GasCharger {
        pub fn new(
            tx_digest: TransactionDigest,
            payment_method: PaymentMethod,
            gas_status: SuiGasStatus,
            protocol_config: &ProtocolConfig,
        ) -> Self {
            let gas_model_version = protocol_config.gas_model_version();
            Self {
                tx_digest,
                gas_model_version,
                payment_method,
                smashed_gas_coin: None,
                gas_status,
            }
        }

        pub fn new_unmetered(tx_digest: TransactionDigest) -> Self {
            Self {
                tx_digest,
                gas_model_version: 6, // pick any of the latest, it should not matter
                payment_method: PaymentMethod::Unmetered,
                smashed_gas_coin: None,
                gas_status: SuiGasStatus::new_unmetered(),
            }
        }

        // TODO: there is only one caller to this function that should not exist otherwise.
        //       Explore way to remove it.
        pub(crate) fn gas_coins(&self) -> impl Iterator<Item = &'_ ObjectRef> {
            match &self.payment_method {
                PaymentMethod::Unmetered => None.iter().chain([].iter()),
                PaymentMethod::Metered {
                    primary_coin,
                    additional_coins,
                    ..
                } => primary_coin.iter().chain(additional_coins.iter()),
            }
        }

        // Return the logical gas coin for this transactions or None if no gas coin was present
        // (system transactions).
        pub fn gas_coin(&self) -> Option<ObjectID> {
            self.smashed_gas_coin
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
        // It will create a synthesized gas coin and smash all gas sources into it.
        // After this call, `gas_coin` will return the id of the synthesized gas coin.
        // This function panics if errors are found while operating on the gas coins.
        // Transaction and certificate input checks must have ensured that all gas coins
        // are correct.
        pub fn smash_gas(
            &mut self,
            tx_ctx: &mut TxContext,
            temporary_store: &mut TemporaryStore<'_>,
        ) {
            let PaymentMethod::Metered {
                primary_coin,
                additional_coins,
                available_address_balance_gas,
                address_balance_payer,
            } = &mut self.payment_method
            else {
                // transaction is unmetered.
                return;
            };

            // 1. Ensure there is a gas coin.
            let (primary_coin_id, primary_coin_balance) = match primary_coin {
                Some(primary_coin) => (
                    primary_coin.0,
                    temporary_store
                        .get_gas_coin_value_unsafe(&primary_coin.0)
                        .unwrap(),
                ),
                None => {
                    let primary_coin_id = tx_ctx.fresh_id();

                    // create object
                    let primary_gas_object = Object::new_move(
                        MoveObject::new_gas_coin(SequenceNumber::new(), primary_coin_id, 0),
                        Owner::AddressOwner(tx_ctx.sender()),
                        tx_ctx.digest(),
                    );
                    temporary_store.create_object(primary_gas_object);
                    (primary_coin_id, 0)
                }
            };

            assert!(primary_coin_id != ObjectID::ZERO);

            // 2. record the primary coin id
            self.smashed_gas_coin = Some(primary_coin_id);

            // 3. Smash additional coins into the primary coin and delete them.
            let total_gas_coin_balance = primary_coin_balance
                + additional_coins
                    .iter()
                    .map(|obj_ref| {
                        // transaction and certificate input checks must have insured that all gas coins
                        // are valid
                        temporary_store.get_gas_coin_value_unsafe(&obj_ref.0)
                            .unwrap_or_else(|_| {
                                panic!(
                                    "Invariant violation: non-gas coin object as input for gas in txn {}",
                                    self.tx_digest
                                )
                            })
                    })
                    .sum();

            for (id, _, _) in additional_coins {
                debug_assert_ne!(*id, primary_coin_id);
                temporary_store.delete_input_object(id);
            }

            // 4. Sweep address balance funds into primary coin
            //    Conservation: Charge against address balance is equal to amount minted into coin.
            temporary_store
                .charge_address_balance_gas(address_balance_payer, *available_address_balance_gas);
            let new_balance = total_gas_coin_balance + *available_address_balance_gas;

            // 6. Set the balance of the primary coin.
            temporary_store
                .set_gas_coin_value_unsafe(&primary_coin_id, new_balance)
                .unwrap_or_else(|_| {
                    panic!(
                        "Invariant violation: failed to set gas coin value in txn {}",
                        self.tx_digest,
                    )
                });
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
        pub fn reset(&mut self, tx_ctx: &mut TxContext, temporary_store: &mut TemporaryStore<'_>) {
            temporary_store.drop_writes();
            self.gas_status.reset_storage_cost_and_rebate();
            self.smash_gas(tx_ctx, temporary_store);
        }

        fn should_write_gas_coin(&self, gas_coin: Option<&Object>) -> bool {
            if let PaymentMethod::Metered {
                primary_coin,
                address_balance_payer,
                ..
            } = &self.payment_method
            {
                primary_coin.is_some()
                    || gas_coin.unwrap().owner() != &Owner::AddressOwner(*address_balance_payer)
            } else {
                false
            }
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
            tx_ctx: &mut TxContext,
            temporary_store: &mut TemporaryStore<'_>,
            execution_result: &mut Result<T, ExecutionError>,
        ) -> GasCostSummary {
            // at this point, we have done *all* charging for computation,
            // but have not yet set the storage rebate or storage gas units
            debug_assert!(self.gas_status.storage_rebate() == 0);
            debug_assert!(self.gas_status.storage_gas_units() == 0);

            let gas_coin = self
                .smashed_gas_coin
                .map(|id| temporary_store.read_object(&id).unwrap().clone());

            let write_gas_coin = self.should_write_gas_coin(gas_coin.as_ref());

            if let PaymentMethod::Metered {
                primary_coin,
                address_balance_payer,
                ..
            } = self.payment_method
            {
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
                    self.reset(tx_ctx, temporary_store);
                }

                if !write_gas_coin {
                    temporary_store.delete_created_object(&gas_coin.as_ref().unwrap().id());
                } else {
                    temporary_store.mutate_input_object(gas_coin.as_ref().unwrap().clone());
                }
            }

            // compute and collect storage charges
            temporary_store.ensure_active_inputs_mutated();
            temporary_store.collect_storage_and_rebate(self);

            let PaymentMethod::Metered {
                address_balance_payer,
                ..
            } = self.payment_method
            else {
                return GasCostSummary::default();
            };

            let mut gas_coin = gas_coin.unwrap();

            self.compute_storage_and_rebate(tx_ctx, temporary_store, execution_result);
            let cost_summary = self.gas_status.summary();
            let net_change: i64 = cost_summary.net_gas_usage();

            deduct_gas(&mut gas_coin, net_change);

            // If the primary coin was initially real, or if it was synthesized but
            // transferred away, then the object must be mutated so it is written
            // in effects.
            if write_gas_coin {
                // no primary coin (address balance payment).
                // Take the remaining balance of the synthesized smashed coin and transfer it back
                // to the address balance.

                // TODO: is this necessary? it should not be because we take pains to exclude the coin
                // from charging if its not going to be written
                let storage_costs = gas_coin.storage_rebate;

                let remaining_balance = get_gas_balance(&gas_coin).unwrap();
                temporary_store.mutate_input_object(gas_coin);
                temporary_store.credit_address_balance_gas(
                    &address_balance_payer,
                    remaining_balance + storage_costs,
                );
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
            tx_ctx: &mut TxContext,
            temporary_store: &mut TemporaryStore<'_>,
            execution_result: &mut Result<T, ExecutionError>,
        ) {
            if dont_charge_budget_on_storage_oog(self.gas_model_version) {
                self.handle_storage_and_rebate_v2(tx_ctx, temporary_store, execution_result)
            } else {
                self.handle_storage_and_rebate_v1(tx_ctx, temporary_store, execution_result)
            }
        }

        fn handle_storage_and_rebate_v1<T>(
            &mut self,
            tx_ctx: &mut TxContext,
            temporary_store: &mut TemporaryStore<'_>,
            execution_result: &mut Result<T, ExecutionError>,
        ) {
            if let Err(err) = self.gas_status.charge_storage_and_rebate() {
                self.reset(tx_ctx, temporary_store);
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
            tx_ctx: &mut TxContext,
            temporary_store: &mut TemporaryStore<'_>,
            execution_result: &mut Result<T, ExecutionError>,
        ) {
            if let Err(err) = self.gas_status.charge_storage_and_rebate() {
                // we run out of gas charging storage, reset and try charging for storage again.
                // Input objects are touched and so they have a storage cost
                // Attempt to charge just for computation + input object storage costs - storage_rebate
                self.reset(tx_ctx, temporary_store);
                temporary_store.ensure_active_inputs_mutated();
                temporary_store.collect_storage_and_rebate(self);
                if let Err(err) = self.gas_status.charge_storage_and_rebate() {
                    // we run out of gas attempting to charge for the input objects exclusively,
                    // deal with this edge case by not charging for storage: we charge (gas_budget - rebates).
                    self.reset(tx_ctx, temporary_store);
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
