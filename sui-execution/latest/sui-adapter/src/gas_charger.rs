// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
pub mod checked {

    use crate::sui_types::gas::SuiGasStatusAPI;
    use crate::temporary_store::TemporaryStore;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::SequenceNumber;
    use sui_types::deny_list_v2::CONFIG_SETTING_DYNAMIC_FIELD_SIZE_FOR_GAS;
    use sui_types::gas::{GasCostSummary, SuiGasStatus};
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
        object::Data,
    };
    use tracing::trace;

    /// Creation number for the synthesized gas coin object.
    /// We use u64::MAX to avoid collision with any user-created objects
    /// (which start from 0 and increment).
    const SYNTHESIZED_GAS_COIN_CREATION_NUM: u64 = u64::MAX;

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
        // All metered transactions use a synthesized gas coin. Real gas coins (if any)
        // and address balance reservations are smashed into this synthesized coin.
        Metered {
            gas_coins: Vec<ObjectRef>, // Real gas coins (may be empty)
            address_balance_payer: SuiAddress,
            available_address_balance_gas: u64, // Sum of all fake coin reservations + pure address balance
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
                PaymentMethod::Unmetered => [].iter(),
                PaymentMethod::Metered { gas_coins, .. } => gas_coins.iter(),
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
        pub fn smash_gas(&mut self, temporary_store: &mut TemporaryStore<'_>) {
            match &self.payment_method {
                PaymentMethod::Unmetered => {}

                PaymentMethod::Metered {
                    gas_coins,
                    address_balance_payer,
                    available_address_balance_gas,
                } => {
                    // 1. Create a synthesized gas coin with zero balance
                    let synth_id =
                        ObjectID::derive_id(self.tx_digest, SYNTHESIZED_GAS_COIN_CREATION_NUM);
                    let gas_coin = MoveObject::new_gas_coin(SequenceNumber::MIN, synth_id, 0);
                    let gas_object = Object::new_move(
                        gas_coin,
                        Owner::AddressOwner(*address_balance_payer),
                        self.tx_digest,
                    );
                    temporary_store.create_object(gas_object);
                    self.smashed_gas_coin = Some(synth_id);

                    // 2. Sum all real gas coins
                    let coin_balance: u64 = gas_coins
                        .iter()
                        .map(|obj_ref| {
                            let obj = temporary_store.objects().get(&obj_ref.0).unwrap_or_else(|| {
                                panic!(
                                    "Invariant violation: gas coin not found in store in txn {}",
                                    self.tx_digest
                                )
                            });
                            let Data::Move(move_obj) = &obj.data else {
                                panic!(
                                    "Invariant violation: non-Move object as input for gas in txn {}",
                                    self.tx_digest
                                );
                            };
                            if !move_obj.type_().is_gas_coin() {
                                panic!(
                                    "Invariant violation: non-gas coin object as input for gas in txn {}",
                                    self.tx_digest
                                );
                            }
                            move_obj.get_coin_value_unsafe()
                        })
                        .sum();

                    // 3. Delete all real gas coins
                    for (id, _, _) in gas_coins.iter() {
                        temporary_store.delete_input_object(id);
                    }

                    // 4. Withdraw from address balance if needed
                    if *available_address_balance_gas > 0 {
                        temporary_store.charge_address_balance_gas(
                            address_balance_payer,
                            *available_address_balance_gas,
                        );
                    }

                    // 5. Set the synthesized coin's balance to the total
                    let new_balance = coin_balance + *available_address_balance_gas;
                    let mut gas_object = temporary_store.read_object(&synth_id).unwrap().clone();
                    gas_object
                        .data
                        .try_as_move_mut()
                        .unwrap()
                        .set_coin_value_unsafe(new_balance);
                    temporary_store.mutate_created_object(gas_object);
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

            if self.smashed_gas_coin.is_some() {
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

            if self.smashed_gas_coin.is_some() {
                #[skip_checked_arithmetic]
                trace!(target: "replay_gas_info", "Gas smashing has occurred for this transaction");
            }

            if self.payment_method.is_unmetered() {
                return GasCostSummary::default();
            }

            self.compute_storage_and_rebate(temporary_store, execution_result);
            let cost_summary = self.gas_status.summary();
            let net_change: i64 = cost_summary.net_gas_usage();

            // Unified path: all metered transactions use synthesized coin
            let PaymentMethod::Metered {
                address_balance_payer,
                ..
            } = &self.payment_method
            else {
                unreachable!()
            };

            let gas_object_id = self.smashed_gas_coin.unwrap();
            let gas_object = temporary_store.read_object(&gas_object_id).unwrap();

            // Determine final owner (may have been transferred during execution!)
            let final_owner = match gas_object.owner {
                Owner::AddressOwner(addr) => addr,
                _ => *address_balance_payer, // fallback if wrapped/shared (shouldn't happen)
            };

            // Get current balance and compute final balance after gas charges
            let current_balance = gas_object
                .data
                .try_as_move()
                .unwrap()
                .get_coin_value_unsafe();
            let final_balance = if net_change < 0 {
                current_balance + (-net_change as u64) // storage rebate
            } else {
                current_balance.saturating_sub(net_change as u64) // gas charge
            };

            // Credit remaining balance to final owner's address balance
            if final_balance > 0 {
                temporary_store.credit_address_balance_gas(&final_owner, final_balance);
            }

            // Emit gas payment from original payer
            temporary_store.emit_net_address_balance_gas_payment(address_balance_payer, net_change);

            // Delete the synthesized coin (balance is now in address balance)
            temporary_store.delete_created_object(&gas_object_id);

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
