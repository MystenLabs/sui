// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use crate::error::{UserInputError, UserInputResult};
    use crate::gas::{self, GasCostSummary, SuiGasStatusAPI};
    use crate::gas_model::gas_predicates::{cost_table_for_version, txn_base_cost_as_multiplier};
    use crate::gas_model::units_types::CostTable;
    use crate::transaction::ObjectReadResult;
    use crate::{
        error::{ExecutionError, ExecutionErrorKind},
        gas_model::tables::{GasStatus, ZERO_COST_SCHEDULE},
        ObjectID,
    };
    use move_core_types::vm_status::StatusCode;
    use sui_protocol_config::*;

    /// A bucket defines a range of units that will be priced the same.
    /// After execution a call to `GasStatus::bucketize` will round the computation
    /// cost to `cost` for the bucket ([`min`, `max`]) the gas used falls into.
    #[allow(dead_code)]
    pub(crate) struct ComputationBucket {
        min: u64,
        max: u64,
        cost: u64,
    }

    impl ComputationBucket {
        fn new(min: u64, max: u64, cost: u64) -> Self {
            ComputationBucket { min, max, cost }
        }

        fn simple(min: u64, max: u64) -> Self {
            Self::new(min, max, max)
        }
    }

    fn get_bucket_cost(table: &[ComputationBucket], computation_cost: u64) -> u64 {
        for bucket in table {
            if bucket.max >= computation_cost {
                return bucket.cost;
            }
        }
        match table.last() {
            // maybe not a literal here could be better?
            None => 5_000_000,
            Some(bucket) => bucket.cost,
        }
    }

    // define the bucket table for computation charging
    // If versioning defines multiple functions and
    fn computation_bucket(max_bucket_cost: u64) -> Vec<ComputationBucket> {
        assert!(max_bucket_cost >= 5_000_000);
        vec![
            ComputationBucket::simple(0, 1_000),
            ComputationBucket::simple(1_000, 5_000),
            ComputationBucket::simple(5_000, 10_000),
            ComputationBucket::simple(10_000, 20_000),
            ComputationBucket::simple(20_000, 50_000),
            ComputationBucket::simple(50_000, 200_000),
            ComputationBucket::simple(200_000, 1_000_000),
            ComputationBucket::simple(1_000_000, max_bucket_cost),
        ]
    }

    /// Portion of the storage rebate that gets passed on to the transaction sender. The remainder
    /// will be burned, then re-minted + added to the storage fund at the next epoch change
    fn sender_rebate(storage_rebate: u64, storage_rebate_rate: u64) -> u64 {
        // we round storage rebate such that `>= x.5` goes to x+1 (rounds up) and
        // `< x.5` goes to x (truncates). We replicate `f32/64::round()`
        const BASIS_POINTS: u128 = 10000;
        (((storage_rebate as u128 * storage_rebate_rate as u128)
        + (BASIS_POINTS / 2)) // integer rounding adds half of the BASIS_POINTS (denominator)
        / BASIS_POINTS) as u64
    }

    /// A list of constant costs of various operations in Sui.
    pub struct SuiCostTable {
        /// A flat fee charged for every transaction. This is also the minimum amount of
        /// gas charged for a transaction.
        pub(crate) min_transaction_cost: u64,
        /// Maximum allowable budget for a transaction.
        pub(crate) max_gas_budget: u64,
        /// Computation cost per byte charged for package publish. This cost is primarily
        /// determined by the cost to verify and link a package. Note that this does not
        /// include the cost of writing the package to the store.
        package_publish_per_byte_cost: u64,
        /// Per byte cost to read objects from the store. This is computation cost instead of
        /// storage cost because it does not change the amount of data stored on the db.
        object_read_per_byte_cost: u64,
        /// Unit cost of a byte in the storage. This will be used both for charging for
        /// new storage as well as rebating for deleting storage. That is, we expect users to
        /// get full refund on the object storage when it's deleted.
        storage_per_byte_cost: u64,
        /// Execution cost table to be used.
        pub execution_cost_table: CostTable,
        /// Computation buckets to cost transaction in price groups
        computation_bucket: Vec<ComputationBucket>,
    }

    impl std::fmt::Debug for SuiCostTable {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            // TODO: dump the fields.
            write!(f, "SuiCostTable(...)")
        }
    }

    impl SuiCostTable {
        pub(crate) fn new(c: &ProtocolConfig, gas_price: u64) -> Self {
            // gas_price here is the Reference Gas Price, however we may decide
            // to change it to be the price passed in the transaction
            let min_transaction_cost = if txn_base_cost_as_multiplier(c) {
                c.base_tx_cost_fixed() * gas_price
            } else {
                c.base_tx_cost_fixed()
            };
            Self {
                min_transaction_cost,
                max_gas_budget: c.max_tx_gas(),
                package_publish_per_byte_cost: c.package_publish_cost_per_byte(),
                object_read_per_byte_cost: c.obj_access_cost_read_per_byte(),
                storage_per_byte_cost: c.obj_data_cost_refundable(),
                execution_cost_table: cost_table_for_version(c.gas_model_version()),
                computation_bucket: computation_bucket(c.max_gas_computation_bucket()),
            }
        }

        pub(crate) fn unmetered() -> Self {
            Self {
                min_transaction_cost: 0,
                max_gas_budget: u64::MAX,
                package_publish_per_byte_cost: 0,
                object_read_per_byte_cost: 0,
                storage_per_byte_cost: 0,
                execution_cost_table: ZERO_COST_SCHEDULE.clone(),
                // should not matter
                computation_bucket: computation_bucket(5_000_000),
            }
        }
    }

    #[derive(Debug)]
    pub struct PerObjectStorage {
        /// storage_cost is the total storage gas to charge. This is computed
        /// at the end of execution while determining storage charges.
        /// It tracks `storage_bytes * obj_data_cost_refundable` as
        /// described in `storage_gas_price`
        /// It has been multiplied by the storage gas price. This is the new storage rebate.
        pub storage_cost: u64,
        /// storage_rebate is the storage rebate (in Sui) for in this object.
        /// This is computed at the end of execution while determining storage charges.
        /// The value is in Sui.
        pub storage_rebate: u64,
        /// The object size post-transaction in bytes
        pub new_size: u64,
    }

    #[allow(dead_code)]
    #[derive(Debug)]
    pub struct SuiGasStatus {
        // GasStatus as used by the VM, that is all the VM sees
        pub gas_status: GasStatus,
        // Cost table contains a set of constant/config for the gas model/charging
        cost_table: SuiCostTable,
        // Gas budget for this gas status instance.
        // Typically the gas budget as defined in the `TransactionData::GasData`
        gas_budget: u64,
        // Computation cost after execution. This is the result of the gas used by the `GasStatus`
        // properly bucketized.
        // Starts at 0 and it is assigned in `bucketize_computation`.
        computation_cost: u64,
        // Whether to charge or go unmetered
        charge: bool,
        // Gas price for computation.
        // This is a multiplier on the final charge as related to the RGP (reference gas price).
        // Checked at signing: `gas_price >= reference_gas_price`
        // and then conceptually
        // `final_computation_cost = total_computation_cost * gas_price / reference_gas_price`
        gas_price: u64,
        // RGP as defined in the protocol config.
        reference_gas_price: u64,
        // Gas price for storage. This is a multiplier on the final charge
        // as related to the storage gas price defined in the system
        // (`ProtocolConfig::storage_gas_price`).
        // Conceptually, given a constant `obj_data_cost_refundable`
        // (defined in `ProtocolConfig::obj_data_cost_refundable`)
        // `total_storage_cost = storage_bytes * obj_data_cost_refundable`
        // `final_storage_cost = total_storage_cost * storage_gas_price`
        storage_gas_price: u64,
        /// Per Object Storage Cost and Storage Rebate, used to get accumulated values at the
        /// end of execution to determine storage charges and rebates.
        per_object_storage: Vec<(ObjectID, PerObjectStorage)>,
        // storage rebate rate as defined in the ProtocolConfig
        rebate_rate: u64,
        /// Amount of storage rebate accumulated when we are running in unmetered mode (i.e. system transaction).
        /// This allows us to track how much storage rebate we need to retain in system transactions.
        unmetered_storage_rebate: u64,
        /// Rounding value to round up gas charges.
        gas_rounding_step: Option<u64>,
    }

    impl SuiGasStatus {
        fn new(
            move_gas_status: GasStatus,
            gas_budget: u64,
            charge: bool,
            gas_price: u64,
            reference_gas_price: u64,
            storage_gas_price: u64,
            rebate_rate: u64,
            gas_rounding_step: Option<u64>,
            cost_table: SuiCostTable,
        ) -> SuiGasStatus {
            let gas_rounding_step = gas_rounding_step.map(|val| val.max(1));
            SuiGasStatus {
                gas_status: move_gas_status,
                gas_budget,
                charge,
                computation_cost: 0,
                gas_price,
                reference_gas_price,
                storage_gas_price,
                per_object_storage: Vec::new(),
                rebate_rate,
                unmetered_storage_rebate: 0,
                gas_rounding_step,
                cost_table,
            }
        }

        pub(crate) fn new_with_budget(
            gas_budget: u64,
            gas_price: u64,
            reference_gas_price: u64,
            config: &ProtocolConfig,
        ) -> SuiGasStatus {
            let storage_gas_price = config.storage_gas_price();
            let max_computation_budget = config.max_gas_computation_bucket() * gas_price;
            let computation_budget = if gas_budget > max_computation_budget {
                max_computation_budget
            } else {
                gas_budget
            };
            let sui_cost_table = SuiCostTable::new(config, gas_price);
            let gas_rounding_step = config.gas_rounding_step_as_option();
            Self::new(
                GasStatus::new(
                    sui_cost_table.execution_cost_table.clone(),
                    computation_budget,
                    gas_price,
                    config.gas_model_version(),
                ),
                gas_budget,
                true,
                gas_price,
                reference_gas_price,
                storage_gas_price,
                config.storage_rebate_rate(),
                gas_rounding_step,
                sui_cost_table,
            )
        }

        pub fn new_unmetered() -> SuiGasStatus {
            Self::new(
                GasStatus::new_unmetered(),
                0,
                false,
                0,
                0,
                0,
                0,
                None,
                SuiCostTable::unmetered(),
            )
        }

        pub fn reference_gas_price(&self) -> u64 {
            self.reference_gas_price
        }

        // Check whether gas arguments are legit:
        // 1. Gas object has an address owner.
        // 2. Gas budget is between min and max budget allowed
        // 3. Gas balance (all gas coins together) is bigger or equal to budget
        pub(crate) fn check_gas_balance(
            &self,
            gas_objs: &[&ObjectReadResult],
            gas_budget: u64,
        ) -> UserInputResult {
            // 1. All gas objects have an address owner
            for gas_object in gas_objs {
                // if as_object() returns None, it means the object has been deleted (and therefore
                // must be a shared object).
                if let Some(obj) = gas_object.as_object() {
                    if !obj.is_address_owned() {
                        return Err(UserInputError::GasObjectNotOwnedObject {
                            owner: obj.owner.clone(),
                        });
                    }
                } else {
                    // This case should never happen (because gas can't be a shared object), but we
                    // handle this case for future-proofing
                    return Err(UserInputError::MissingGasPayment);
                }
            }

            // 2. Gas budget is between min and max budget allowed
            if gas_budget > self.cost_table.max_gas_budget {
                return Err(UserInputError::GasBudgetTooHigh {
                    gas_budget,
                    max_budget: self.cost_table.max_gas_budget,
                });
            }
            if gas_budget < self.cost_table.min_transaction_cost {
                return Err(UserInputError::GasBudgetTooLow {
                    gas_budget,
                    min_budget: self.cost_table.min_transaction_cost,
                });
            }

            // 3. Gas balance (all gas coins together) is bigger or equal to budget
            let mut gas_balance = 0u128;
            for gas_obj in gas_objs {
                // expect is safe because we already checked that all gas objects have an address owner
                gas_balance +=
                    gas::get_gas_balance(gas_obj.as_object().expect("object must be owned"))?
                        as u128;
            }
            if gas_balance < gas_budget as u128 {
                Err(UserInputError::GasBalanceTooLow {
                    gas_balance,
                    needed_gas_amount: gas_budget as u128,
                })
            } else {
                Ok(())
            }
        }

        fn storage_cost(&self) -> u64 {
            self.storage_gas_units()
        }

        pub fn per_object_storage(&self) -> &Vec<(ObjectID, PerObjectStorage)> {
            &self.per_object_storage
        }
    }

    impl SuiGasStatusAPI for SuiGasStatus {
        fn is_unmetered(&self) -> bool {
            !self.charge
        }

        fn move_gas_status(&self) -> &GasStatus {
            &self.gas_status
        }

        fn move_gas_status_mut(&mut self) -> &mut GasStatus {
            &mut self.gas_status
        }

        fn bucketize_computation(&mut self) -> Result<(), ExecutionError> {
            let gas_used = self.gas_status.gas_used_pre_gas_price();
            let gas_used = if let Some(gas_rounding) = self.gas_rounding_step {
                if gas_used > 0 && gas_used % gas_rounding == 0 {
                    gas_used * self.gas_price
                } else {
                    ((gas_used / gas_rounding) + 1) * gas_rounding * self.gas_price
                }
            } else {
                let bucket_cost = get_bucket_cost(&self.cost_table.computation_bucket, gas_used);
                // charge extra on top of `computation_cost` to make the total computation
                // cost a bucket value
                bucket_cost * self.gas_price
            };
            if self.gas_budget <= gas_used {
                self.computation_cost = self.gas_budget;
                Err(ExecutionErrorKind::InsufficientGas.into())
            } else {
                self.computation_cost = gas_used;
                Ok(())
            }
        }

        /// Returns the final (computation cost, storage cost, storage rebate) of the gas meter.
        /// We use initial budget, combined with remaining gas and storage cost to derive
        /// computation cost.
        fn summary(&self) -> GasCostSummary {
            // compute storage rebate, both rebate and non refundable fee
            let storage_rebate = self.storage_rebate();
            let sender_rebate = sender_rebate(storage_rebate, self.rebate_rate);
            assert!(sender_rebate <= storage_rebate);
            let non_refundable_storage_fee = storage_rebate - sender_rebate;
            GasCostSummary {
                computation_cost: self.computation_cost,
                storage_cost: self.storage_cost(),
                storage_rebate: sender_rebate,
                non_refundable_storage_fee,
            }
        }

        fn gas_budget(&self) -> u64 {
            self.gas_budget
        }

        fn storage_gas_units(&self) -> u64 {
            self.per_object_storage
                .iter()
                .map(|(_, per_object)| per_object.storage_cost)
                .sum()
        }

        fn storage_rebate(&self) -> u64 {
            self.per_object_storage
                .iter()
                .map(|(_, per_object)| per_object.storage_rebate)
                .sum()
        }

        fn unmetered_storage_rebate(&self) -> u64 {
            self.unmetered_storage_rebate
        }

        fn gas_used(&self) -> u64 {
            self.gas_status.gas_used_pre_gas_price()
        }

        fn reset_storage_cost_and_rebate(&mut self) {
            self.per_object_storage = Vec::new();
            self.unmetered_storage_rebate = 0;
        }

        fn charge_storage_read(&mut self, size: usize) -> Result<(), ExecutionError> {
            self.gas_status
                .charge_bytes(size, self.cost_table.object_read_per_byte_cost)
                .map_err(|e| {
                    debug_assert_eq!(e.major_status(), StatusCode::OUT_OF_GAS);
                    ExecutionErrorKind::InsufficientGas.into()
                })
        }

        fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError> {
            self.gas_status
                .charge_bytes(size, self.cost_table.package_publish_per_byte_cost)
                .map_err(|e| {
                    debug_assert_eq!(e.major_status(), StatusCode::OUT_OF_GAS);
                    ExecutionErrorKind::InsufficientGas.into()
                })
        }

        /// Update `storage_rebate` and `storage_gas_units` for each object in the transaction.
        /// There is no charge in this function. Charges will all be applied together at the end
        /// (`track_storage_mutation`).
        /// Return the new storage rebate (cost of object storage) according to `new_size`.
        fn track_storage_mutation(
            &mut self,
            object_id: ObjectID,
            new_size: usize,
            storage_rebate: u64,
        ) -> u64 {
            if self.is_unmetered() {
                self.unmetered_storage_rebate += storage_rebate;
                return 0;
            }

            // compute and track cost (based on size)
            let new_size = new_size as u64;
            let storage_cost =
                new_size * self.cost_table.storage_per_byte_cost * self.storage_gas_price;
            // track rebate

            self.per_object_storage.push((
                object_id,
                PerObjectStorage {
                    storage_cost,
                    storage_rebate,
                    new_size,
                },
            ));
            // return the new object rebate (object storage cost)
            storage_cost
        }

        fn charge_storage_and_rebate(&mut self) -> Result<(), ExecutionError> {
            let storage_rebate = self.storage_rebate();
            let storage_cost = self.storage_cost();
            let sender_rebate = sender_rebate(storage_rebate, self.rebate_rate);
            assert!(sender_rebate <= storage_rebate);
            if sender_rebate >= storage_cost {
                // there is more rebate than cost, when deducting gas we are adding
                // to whatever is the current amount charged so we are `Ok`
                Ok(())
            } else {
                let gas_left = self.gas_budget - self.computation_cost;
                // we have to charge for storage and may go out of gas, check
                if gas_left < storage_cost - sender_rebate {
                    // Running out of gas would cause the temporary store to reset
                    // and zero storage and rebate.
                    // The remaining_gas will be 0 and we will charge all in computation
                    Err(ExecutionErrorKind::InsufficientGas.into())
                } else {
                    Ok(())
                }
            }
        }

        fn adjust_computation_on_out_of_gas(&mut self) {
            self.per_object_storage = Vec::new();
            self.computation_cost = self.gas_budget;
        }
    }
}
