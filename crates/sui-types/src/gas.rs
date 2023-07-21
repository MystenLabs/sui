// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
pub mod checked {

    use crate::gas_model::gas_predicates::{dont_charge_budget_on_storage_oog, gas_price_too_high};
    use crate::{
        base_types::{ObjectID, ObjectRef},
        digests::TransactionDigest,
        effects::{TransactionEffects, TransactionEffectsAPI},
        error::{ExecutionError, SuiResult, UserInputError, UserInputResult},
        gas_model::{gas_v2::SuiGasStatus as SuiGasStatusV2, tables::GasStatus},
        is_system_package,
        object::{Data, Object},
        storage::{DeleteKindWithOldVersion, WriteKind},
        sui_serde::{BigInt, Readable},
        temporary_store::TemporaryStore,
    };
    use enum_dispatch::enum_dispatch;
    use itertools::MultiUnzip;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_with::serde_as;
    use sui_protocol_config::ProtocolConfig;
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
        gas_coins: Vec<ObjectRef>,
        // this is the the first gas coin in `gas_coins` and the one that all others will
        // be smashed into. It can be None for system transactions when `gas_coins` is empty.
        smashed_gas_coin: Option<ObjectID>,
        gas_status: SuiGasStatus,
    }

    impl GasCharger {
        pub fn new(
            tx_digest: TransactionDigest,
            gas_coins: Vec<ObjectRef>,
            gas_status: SuiGasStatus,
            protocol_config: &ProtocolConfig,
        ) -> Self {
            let gas_model_version = protocol_config.gas_model_version();
            Self {
                tx_digest,
                gas_model_version,
                gas_coins,
                smashed_gas_coin: None,
                gas_status,
            }
        }

        pub fn new_unmetered(tx_digest: TransactionDigest) -> Self {
            Self {
                tx_digest,
                gas_model_version: 6, // pick any of the latest, it should not matter
                gas_coins: vec![],
                smashed_gas_coin: None,
                gas_status: SuiGasStatus::new_unmetered(),
            }
        }

        // TODO: there is only one caller to this function that should not exist otherwise.
        //       Explore way to remove it.
        pub(crate) fn gas_coins(&self) -> &[ObjectRef] {
            &self.gas_coins
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
            let gas_coin_count = self.gas_coins.len();
            if gas_coin_count == 0 || (gas_coin_count == 1 && self.gas_coins[0].0 == ObjectID::ZERO)
            {
                return; // self.smashed_gas_coin is None
            }
            // set the first coin to be the transaction only gas coin.
            // All others will be smashed into this one.
            let gas_coin_id = self.gas_coins[0].0;
            self.smashed_gas_coin = Some(gas_coin_id);
            if gas_coin_count == 1 {
                return;
            }
            // sum the value of all gas coins
            let new_balance = self
                .gas_coins
                .iter()
                .map(|obj_ref| {
                    let obj = temporary_store.objects().get(&obj_ref.0).unwrap();
                    let Data::Move(move_obj) = &obj.data else {
                    return Err(ExecutionError::invariant_violation(
                        "Provided non-gas coin object as input for gas!"
                    ));
                };
                    if !move_obj.type_().is_gas_coin() {
                        return Err(ExecutionError::invariant_violation(
                            "Provided non-gas coin object as input for gas!",
                        ));
                    }
                    Ok(move_obj.get_coin_value_unsafe())
                })
                .collect::<Result<Vec<u64>, ExecutionError>>()
                // transaction and certificate input checks must have insured that all gas coins
                // are valid
                .unwrap_or_else(|_| {
                    panic!(
                        "Invariant violation: non-gas coin object as input for gas in txn {}",
                        self.tx_digest
                    )
                })
                .iter()
                .sum();
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
            // delete all gas objects except the primary_gas_object
            for (id, version, _digest) in &self.gas_coins[1..] {
                debug_assert_ne!(*id, primary_gas_object.id());
                temporary_store.delete_object(id, DeleteKindWithOldVersion::Normal(*version));
            }
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
                .set_coin_value_unsafe(new_balance);
            temporary_store.write_object(primary_gas_object, WriteKind::Mutate);
        }

        //
        // Gas charging operations
        //

        pub fn track_storage_mutation(&mut self, new_size: usize, storage_rebate: u64) -> u64 {
            self.gas_status
                .track_storage_mutation(new_size, storage_rebate)
        }

        pub fn reset_storage_cost_and_rebate(&mut self) {
            self.gas_status.reset_storage_cost_and_rebate();
        }

        pub fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError> {
            self.gas_status.charge_publish_package(size)
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
                if let Err(err) = self.gas_status.bucketize_computation() {
                    if execution_result.is_ok() {
                        *execution_result = Err(err);
                    }
                }

                // On error we need to dump writes, deletes, etc before charging storage gas
                if execution_result.is_err() {
                    self.reset(temporary_store);
                }
            }

            // compute and collect storage charges
            temporary_store.ensure_gas_and_input_mutated(self);
            temporary_store.collect_storage_and_rebate(self);

            // system transactions (None smashed_gas_coin)  do not have gas and so do not charge
            // for storage, however they track storage values to check for conservation rules
            if let Some(gas_object_id) = self.smashed_gas_coin {
                if dont_charge_budget_on_storage_oog(self.gas_model_version) {
                    self.handle_storage_and_rebate_v2(temporary_store, execution_result)
                } else {
                    self.handle_storage_and_rebate_v1(temporary_store, execution_result)
                }

                let cost_summary = self.gas_status.summary();
                let gas_used = cost_summary.net_gas_usage();

                let mut gas_object = temporary_store.read_object(&gas_object_id).unwrap().clone();
                deduct_gas(&mut gas_object, gas_used);
                #[skip_checked_arithmetic]
                trace!(gas_used, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");

                temporary_store.write_object(gas_object, WriteKind::Mutate);
                cost_summary
            } else {
                GasCostSummary::default()
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
                temporary_store.ensure_gas_and_input_mutated(self);
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
                self.reset(temporary_store);
                temporary_store.ensure_gas_and_input_mutated(self);
                temporary_store.collect_storage_and_rebate(self);
                if let Err(err) = self.gas_status.charge_storage_and_rebate() {
                    // we run out of gas attempting to charge for the input objects exclusively,
                    // deal with this edge case by not charging for storage
                    self.reset(temporary_store);
                    self.gas_status.adjust_computation_on_out_of_gas();
                    temporary_store.ensure_gas_and_input_mutated(self);
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

    #[enum_dispatch]
    pub(crate) trait SuiGasStatusAPI {
        fn is_unmetered(&self) -> bool;
        fn move_gas_status(&self) -> &GasStatus;
        fn move_gas_status_mut(&mut self) -> &mut GasStatus;
        fn bucketize_computation(&mut self) -> Result<(), ExecutionError>;
        fn summary(&self) -> GasCostSummary;
        fn gas_budget(&self) -> u64;
        fn storage_gas_units(&self) -> u64;
        fn storage_rebate(&self) -> u64;
        fn unmetered_storage_rebate(&self) -> u64;
        fn gas_used(&self) -> u64;
        fn reset_storage_cost_and_rebate(&mut self);
        fn charge_storage_read(&mut self, size: usize) -> Result<(), ExecutionError>;
        fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError>;
        fn track_storage_mutation(&mut self, new_size: usize, storage_rebate: u64) -> u64;
        fn charge_storage_and_rebate(&mut self) -> Result<(), ExecutionError>;
        fn adjust_computation_on_out_of_gas(&mut self);
    }

    /// Version aware enum for gas status.
    #[enum_dispatch(SuiGasStatusAPI)]
    #[derive(Debug)]
    pub enum SuiGasStatus {
        // V1 does not exists any longer as it was a pre mainnet version.
        // So we start the enum from V2
        V2(SuiGasStatusV2),
    }

    impl SuiGasStatus {
        pub fn new(
            gas_budget: u64,
            gas_price: u64,
            reference_gas_price: u64,
            config: &ProtocolConfig,
        ) -> SuiResult<Self> {
            // Common checks. We may pull them into version specific status as needed, but they
            // are unlikely to change.

            // gas price must be bigger or equal to reference gas price
            if gas_price < reference_gas_price {
                return Err(UserInputError::GasPriceUnderRGP {
                    gas_price,
                    reference_gas_price,
                }
                .into());
            }
            if gas_price_too_high(config.gas_model_version()) && gas_price >= config.max_gas_price()
            {
                return Err(UserInputError::GasPriceTooHigh {
                    max_gas_price: config.max_gas_price(),
                }
                .into());
            }

            Ok(Self::V2(SuiGasStatusV2::new_with_budget(
                gas_budget,
                gas_price,
                reference_gas_price,
                config,
            )))
        }

        pub fn new_unmetered() -> Self {
            Self::V2(SuiGasStatusV2::new_unmetered())
        }

        // This is the only public API on SuiGasStatus, all other gas related operations should
        // go through `GasCharger`
        pub fn check_gas_balance(&self, gas_objs: &[&Object], gas_budget: u64) -> UserInputResult {
            match self {
                Self::V2(status) => status.check_gas_balance(gas_objs, gas_budget),
            }
        }
    }

    /// Summary of the charges in a transaction.
    /// Storage is charged independently of computation.
    /// There are 3 parts to the storage charges:
    /// `storage_cost`: it is the charge of storage at the time the transaction is executed.
    ///                 The cost of storage is the number of bytes of the objects being mutated
    ///                 multiplied by a variable storage cost per byte
    /// `storage_rebate`: this is the amount a user gets back when manipulating an object.
    ///                   The `storage_rebate` is the `storage_cost` for an object minus fees.
    /// `non_refundable_storage_fee`: not all the value of the object storage cost is
    ///                               given back to user and there is a small fraction that
    ///                               is kept by the system. This value tracks that charge.
    ///
    /// When looking at a gas cost summary the amount charged to the user is
    /// `computation_cost + storage_cost - storage_rebate`
    /// and that is the amount that is deducted from the gas coins.
    /// `non_refundable_storage_fee` is collected from the objects being mutated/deleted
    /// and it is tracked by the system in storage funds.
    ///
    /// Objects deleted, including the older versions of objects mutated, have the storage field
    /// on the objects added up to a pool of "potential rebate". This rebate then is reduced
    /// by the "nonrefundable rate" such that:
    /// `potential_rebate(storage cost of deleted/mutated objects) =
    /// storage_rebate + non_refundable_storage_fee`

    #[serde_as]
    #[derive(Eq, PartialEq, Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct GasCostSummary {
        /// Cost of computation/execution
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "Readable<BigInt<u64>, _>")]
        pub computation_cost: u64,
        /// Storage cost, it's the sum of all storage cost for all objects created or mutated.
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "Readable<BigInt<u64>, _>")]
        pub storage_cost: u64,
        /// The amount of storage cost refunded to the user for all objects deleted or mutated in the
        /// transaction.
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "Readable<BigInt<u64>, _>")]
        pub storage_rebate: u64,
        /// The fee for the rebate. The portion of the storage rebate kept by the system.
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "Readable<BigInt<u64>, _>")]
        pub non_refundable_storage_fee: u64,
    }

    impl GasCostSummary {
        pub fn new(
            computation_cost: u64,
            storage_cost: u64,
            storage_rebate: u64,
            non_refundable_storage_fee: u64,
        ) -> GasCostSummary {
            GasCostSummary {
                computation_cost,
                storage_cost,
                storage_rebate,
                non_refundable_storage_fee,
            }
        }

        pub fn gas_used(&self) -> u64 {
            self.computation_cost + self.storage_cost
        }

        /// Portion of the storage rebate that gets passed on to the transaction sender. The remainder
        /// will be burned, then re-minted + added to the storage fund at the next epoch change
        pub fn sender_rebate(&self, storage_rebate_rate: u64) -> u64 {
            // we round storage rebate such that `>= x.5` goes to x+1 (rounds up) and
            // `< x.5` goes to x (truncates). We replicate `f32/64::round()`
            const BASIS_POINTS: u128 = 10000;
            (((self.storage_rebate as u128 * storage_rebate_rate as u128)
            + (BASIS_POINTS / 2)) // integer rounding adds half of the BASIS_POINTS (denominator)
            / BASIS_POINTS) as u64
        }

        /// Get net gas usage, positive number means used gas; negative number means refund.
        pub fn net_gas_usage(&self) -> i64 {
            self.gas_used() as i64 - self.storage_rebate as i64
        }

        pub fn new_from_txn_effects<'a>(
            transactions: impl Iterator<Item = &'a TransactionEffects>,
        ) -> GasCostSummary {
            let (storage_costs, computation_costs, storage_rebates, non_refundable_storage_fee): (
                Vec<u64>,
                Vec<u64>,
                Vec<u64>,
                Vec<u64>,
            ) = transactions
                .map(|e| {
                    (
                        e.gas_cost_summary().storage_cost,
                        e.gas_cost_summary().computation_cost,
                        e.gas_cost_summary().storage_rebate,
                        e.gas_cost_summary().non_refundable_storage_fee,
                    )
                })
                .multiunzip();

            GasCostSummary {
                storage_cost: storage_costs.iter().sum(),
                computation_cost: computation_costs.iter().sum(),
                storage_rebate: storage_rebates.iter().sum(),
                non_refundable_storage_fee: non_refundable_storage_fee.iter().sum(),
            }
        }
    }

    impl std::fmt::Display for GasCostSummary {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
            f,
            "computation_cost: {}, storage_cost: {},  storage_rebate: {}, non_refundable_storage_fee: {}",
            self.computation_cost, self.storage_cost, self.storage_rebate, self.non_refundable_storage_fee,
        )
        }
    }

    //
    // Helper functions to deal with gas coins operations.
    //

    pub fn deduct_gas(gas_object: &mut Object, charge_or_rebate: i64) {
        // The object must be a gas coin as we have checked in transaction handle phase.
        let gas_coin = gas_object.data.try_as_move_mut().unwrap();
        let balance = gas_coin.get_coin_value_unsafe();
        let new_balance = if charge_or_rebate < 0 {
            balance + (-charge_or_rebate as u64)
        } else {
            assert!(balance >= charge_or_rebate as u64);
            balance - charge_or_rebate as u64
        };
        gas_coin.set_coin_value_unsafe(new_balance)
    }

    pub fn get_gas_balance(gas_object: &Object) -> UserInputResult<u64> {
        if let Some(move_obj) = gas_object.data.try_as_move() {
            if !move_obj.type_().is_gas_coin() {
                return Err(UserInputError::InvalidGasObject {
                    object_id: gas_object.id(),
                });
            }
            Ok(move_obj.get_coin_value_unsafe())
        } else {
            Err(UserInputError::InvalidGasObject {
                object_id: gas_object.id(),
            })
        }
    }
}
