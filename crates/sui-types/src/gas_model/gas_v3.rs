// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

mod checked {
    use crate::error::UserInputResult;
    use crate::gas::{GasCostSummary, GasUsageReport, SuiGasStatusAPI};
    use crate::gas_model::gas_common::{
        StorageGas, check_gas_data, check_gas_objects, half_digits_rounding, sender_rebate,
    };
    use crate::gas_model::gas_predicates::cost_table_for_version;
    use crate::gas_model::units_types::CostTable;
    use crate::transaction::ObjectReadResult;
    use crate::{
        ObjectID,
        error::ExecutionError,
        execution_status::ExecutionErrorKind,
        gas_model::tables::{GasStatus, ZERO_COST_SCHEDULE},
    };
    use move_core_types::vm_status::StatusCode;
    use sui_protocol_config::*;

    /// A list of constant costs of various operations in Sui.
    pub struct SuiCostTable {
        /// A flat fee charged for every transaction.
        pub(crate) min_transaction_cost: u64,
        /// Maximum allowable budget for a transaction.
        pub(crate) max_gas_budget: u64,
        /// Computation cost per byte charged for package publish.
        package_publish_per_byte_cost: u64,
        /// Per byte cost to read objects from the store.
        object_read_per_byte_cost: u64,
        /// Unit cost of a byte in the storage.
        storage_per_byte_cost: u64,
        /// Execution cost table to be used.
        pub execution_cost_table: CostTable,
        /// RGP-multiplier cap on the effective gas price for aborted transactions.
        max_gas_price_rgp_factor_for_aborted_transactions: Option<u64>,
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
            let min_transaction_cost = c
                .base_tx_cost_fixed()
                .checked_mul(gas_price)
                .expect("base tx cost cannot overflow: gas_price is bounded by max_gas_price");
            Self {
                min_transaction_cost,
                max_gas_budget: c.max_tx_gas(),
                package_publish_per_byte_cost: c.package_publish_cost_per_byte(),
                object_read_per_byte_cost: c.obj_access_cost_read_per_byte(),
                storage_per_byte_cost: c.obj_data_cost_refundable(),
                execution_cost_table: cost_table_for_version(c.gas_model_version()),
                max_gas_price_rgp_factor_for_aborted_transactions: c
                    .max_gas_price_rgp_factor_for_aborted_transactions_as_option(),
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
                max_gas_price_rgp_factor_for_aborted_transactions: None,
            }
        }
    }

    pub use crate::gas_model::gas_common::PerObjectStorage;

    #[allow(dead_code)]
    #[derive(Debug)]
    pub struct SuiGasStatus {
        // GasStatus as used by the VM, that is all the VM sees
        pub gas_status: GasStatus,
        // Cost table contains a set of constant/config for the gas model/charging
        cost_table: SuiCostTable,
        // Gas budget for this gas status instance.
        gas_budget: u64,
        // Whether to charge or go unmetered
        charge: bool,
        // Price used to convert gas units to MIST; starts at `user_gas_price`, lowered to the abort cap
        // by `bucketize_computation` on a Move abort.
        effective_gas_price: u64,
        // The gas price the user signed for (>= reference_gas_price).
        user_gas_price: u64,
        // RGP as defined in the protocol config.
        reference_gas_price: u64,
        // storage rebate rate as defined in the ProtocolConfig
        rebate_rate: u64,
        /// Per-object storage accounting .
        storage: StorageGas,
        /// When set, computation cost is reported as exactly `gas_budget`.
        force_computation_cost_to_budget: bool,
    }

    impl SuiGasStatus {
        fn new(
            move_gas_status: GasStatus,
            gas_budget: u64,
            charge: bool,
            user_gas_price: u64,
            reference_gas_price: u64,
            storage_gas_price: u64,
            rebate_rate: u64,
            cost_table: SuiCostTable,
        ) -> SuiGasStatus {
            SuiGasStatus {
                gas_status: move_gas_status,
                gas_budget,
                charge,
                effective_gas_price: user_gas_price,
                user_gas_price,
                reference_gas_price,
                rebate_rate,
                storage: StorageGas::new(storage_gas_price, cost_table.storage_per_byte_cost),
                cost_table,
                force_computation_cost_to_budget: false,
            }
        }

        pub(crate) fn new_with_budget(
            gas_budget: u64,
            gas_price: u64,
            reference_gas_price: u64,
            config: &ProtocolConfig,
        ) -> SuiGasStatus {
            let storage_gas_price = config.storage_gas_price();
            let max_computation_budget = config
                .max_gas_computation_bucket()
                .checked_mul(gas_price)
                .expect(
                    "computation budget cannot overflow: gas_price is bounded by max_gas_price",
                );
            let computation_budget = if gas_budget > max_computation_budget {
                max_computation_budget
            } else {
                gas_budget
            };
            let sui_cost_table = SuiCostTable::new(config, gas_price);
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
                SuiCostTable::unmetered(),
            )
        }

        pub fn reference_gas_price(&self) -> u64 {
            self.reference_gas_price
        }

        /// Meter-derived computation cost in MIST (bucketed units × effective_gas_price).
        fn uncapped_computation_cost(&self) -> u64 {
            if self.force_computation_cost_to_budget {
                return self.gas_budget;
            }
            let raw_units = self.gas_status.gas_used_pre_gas_price();
            let bucketed_units = half_digits_rounding(raw_units);
            bucketed_units.saturating_mul(self.effective_gas_price)
        }

        /// Computation cost reported in `summary()`, capped so
        /// `computation + storage_cost - sender_rebate ≤ gas_budget`
        /// storage charges stick, computation absorbs whatever budget remains.
        fn derived_computation_cost(&self) -> u64 {
            let uncapped_cost = self.uncapped_computation_cost();
            let storage_rebate = self.storage_rebate();
            let sender_rebate = sender_rebate(storage_rebate, self.rebate_rate);
            let net_storage = self.storage_cost().saturating_sub(sender_rebate);
            let max_computation = self.gas_budget.saturating_sub(net_storage);
            uncapped_cost.min(max_computation)
        }

        fn storage_cost(&self) -> u64 {
            self.storage_gas_units()
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

        fn bucketize_computation(&mut self, aborted: Option<bool>) -> Result<(), ExecutionError> {
            self.effective_gas_price = match self
                .cost_table
                .max_gas_price_rgp_factor_for_aborted_transactions
            {
                Some(factor) if aborted.unwrap_or(false) => {
                    let cap = factor
                        .checked_mul(self.reference_gas_price)
                        .ok_or_else(|| {
                            ExecutionError::from_kind(ExecutionErrorKind::InvariantViolation)
                        })?;
                    self.user_gas_price.min(cap)
                }
                _ => self.user_gas_price,
            };
            if self.uncapped_computation_cost() >= self.gas_budget {
                return Err(ExecutionErrorKind::InsufficientGas.into());
            }
            Ok(())
        }

        /// Returns the gas cost summary for the transaction.
        fn summary(&self) -> GasCostSummary {
            let storage_rebate = self.storage_rebate();
            let sender_rebate = sender_rebate(storage_rebate, self.rebate_rate);
            assert!(sender_rebate <= storage_rebate);
            let non_refundable_storage_fee = storage_rebate - sender_rebate;
            GasCostSummary {
                computation_cost: self.derived_computation_cost(),
                storage_cost: self.storage_cost(),
                storage_rebate: sender_rebate,
                non_refundable_storage_fee,
            }
        }

        fn gas_budget(&self) -> u64 {
            self.gas_budget
        }

        fn gas_price(&self) -> u64 {
            self.user_gas_price
        }

        fn reference_gas_price(&self) -> u64 {
            self.reference_gas_price
        }

        fn storage_gas_units(&self) -> u64 {
            self.storage.storage_gas_units()
        }

        fn storage_rebate(&self) -> u64 {
            self.storage.storage_rebate()
        }

        fn unmetered_storage_rebate(&self) -> u64 {
            self.storage.unmetered_storage_rebate()
        }

        fn gas_used(&self) -> u64 {
            self.gas_status.gas_used_pre_gas_price()
        }

        fn reset_storage_cost_and_rebate(&mut self) {
            self.storage.reset();
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
        /// Return the new storage rebate (cost of object storage) according to `new_size` or
        /// `None` on arithmetic errors.
        fn track_storage_mutation(
            &mut self,
            object_id: ObjectID,
            new_size: usize,
            storage_rebate: u64,
        ) -> Option<u64> {
            let unmetered = self.is_unmetered();
            self.storage
                .track_mutation(object_id, new_size, storage_rebate, unmetered)
        }

        fn charge_storage_and_rebate(&mut self) -> Result<(), ExecutionError> {
            let storage_rebate = self.storage.storage_rebate();
            let storage_cost = self.storage.storage_gas_units();
            let sender_rebate = sender_rebate(storage_rebate, self.rebate_rate);
            assert!(sender_rebate <= storage_rebate);
            let net_storage_cost = storage_cost.saturating_sub(sender_rebate);
            let gas_left = self
                .gas_budget
                .saturating_sub(self.uncapped_computation_cost());
            if net_storage_cost > gas_left {
                return Err(ExecutionErrorKind::InsufficientGas.into());
            }
            Ok(())
        }

        /// Drop accumulated storage and force `summary()` to report `computation_cost == gas_budget`
        fn adjust_computation_on_out_of_gas(&mut self) {
            self.storage.reset();
            self.force_computation_cost_to_budget = true;
        }

        fn gas_usage_report(&self) -> GasUsageReport {
            GasUsageReport {
                cost_summary: self.summary(),
                gas_used: self.gas_used(),
                gas_price: self.gas_price(),
                reference_gas_price: self.reference_gas_price(),
                per_object_storage: self.per_object_storage().clone(),
                gas_budget: self.gas_budget(),
                storage_gas_price: self.storage.storage_gas_price,
                rebate_rate: self.rebate_rate,
            }
        }

        // Check whether gas arguments are legit:
        // 1. Gas object has an address owner.
        // 2. Gas budget is between min and max budget allowed
        // 3. Gas balance (all gas coins together) is bigger or equal to budget
        fn check_gas_balance(
            &self,
            gas_objs: &[&ObjectReadResult],
            gas_budget: u64,
            available_address_balance_gas: u64,
        ) -> UserInputResult {
            self.check_gas_objects(gas_objs)?;
            check_gas_data(
                gas_objs,
                gas_budget,
                available_address_balance_gas,
                self.cost_table.min_transaction_cost,
                self.cost_table.max_gas_budget,
            )
        }

        fn check_gas_objects(&self, gas_objs: &[&ObjectReadResult]) -> UserInputResult {
            check_gas_objects(gas_objs)
        }

        fn per_object_storage(&self) -> &Vec<(ObjectID, PerObjectStorage)> {
            self.storage.per_object_storage()
        }
    }
}
