// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use crate::error::{UserInputError, UserInputResult};
    use crate::gas::{self, GasCostSummary, GasUsageReport, SuiGasStatusAPI};
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
        /// RGP-multiplier cap on the effective gas price for aborted transactions.
        /// gas_v3 only runs at gas_model >= 15 (protocol v127+), where this field is always
        /// `Some(_)` in the protocol config — stored as a plain `u64` here.
        max_gas_price_rgp_factor_for_aborted_transactions: u64,
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
            //
            // `txn_base_cost_as_multiplier` is on at every protocol version this gas_v3 runs at
            // (v127+), so the base cost is always scaled by gas_price.
            let min_transaction_cost = c.base_tx_cost_fixed() * gas_price;
            Self {
                min_transaction_cost,
                max_gas_budget: c.max_tx_gas(),
                package_publish_per_byte_cost: c.package_publish_cost_per_byte(),
                object_read_per_byte_cost: c.obj_access_cost_read_per_byte(),
                storage_per_byte_cost: c.obj_data_cost_refundable(),
                execution_cost_table: cost_table_for_version(c.gas_model_version()),
                max_gas_price_rgp_factor_for_aborted_transactions: c
                    .max_gas_price_rgp_factor_for_aborted_transactions(),
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
                // Unmetered txs never enter `bucketize_computation`, so this value is never
                // observed; 0 is fine as a placeholder.
                max_gas_price_rgp_factor_for_aborted_transactions: 0,
            }
        }
    }

    // gas_v3 reuses the shared `PerObjectStorage` from gas_v2 — it is a pure data type with
    // no behavioural difference between the two pipelines, and is exposed through
    // `GasUsageReport.per_object_storage` to callers like sui-replay.
    pub use crate::gas_model::gas_v2::PerObjectStorage;

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
        // Whether to charge or go unmetered
        charge: bool,
        // The price `summary()` uses to convert raw gas units into MIST. Starts
        // equal to `user_gas_price`; lowered to the abort-tx cap by
        // `bucketize_computation` when the result is a Move abort and the
        // protocol config sets `max_gas_price_rgp_factor_for_aborted_transactions`.
        effective_gas_price: u64,
        // The gas price the user signed for. Checked at signing
        // (`user_gas_price >= reference_gas_price`). Used as the upper bound
        // for the abort cap, as the initial value of `effective_gas_price`,
        // and exposed via the `gas_price()` accessor for TxContext / RPC /
        // telemetry.
        user_gas_price: u64,
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
        /// When true, `uncapped_computation_cost` and `derived_computation_cost`
        /// short-circuit to `gas_budget` instead of deriving from the Move VM
        /// meter. Set by `adjust_computation_on_out_of_gas` on the err-path
        /// fallback (legacy storage-OOG handler, step-pipeline
        /// `set_computation_to_budget`) so the gas summary reports the budget
        /// exactly — meter-derived math is `(budget / user_gas_price) ×
        /// effective_gas_price` and loses up to `budget % user_gas_price`
        /// MIST to integer-division floor.
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
                storage_gas_price,
                per_object_storage: Vec::new(),
                rebate_rate,
                unmetered_storage_rebate: 0,
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
            // `gas_rounding_halve_digits` is on at every protocol version this gas_v3 runs at
            // (v127+), so rounding is always `half_digits_rounding` in `uncapped_computation_cost`.
            // No enum, no per-version branch.
            let storage_gas_price = config.storage_gas_price();
            let max_computation_budget = config.max_gas_computation_bucket() * gas_price;
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

        /// Current computation cost in MIST derived from the Move VM meter:
        ///   `gas_used_pre_gas_price() × effective_gas_price`,
        /// capped so that `computation_cost + storage_cost - sender_rebate ≤ gas_budget`.
        ///
        /// `effective_gas_price` starts equal to the user's `user_gas_price`
        /// and is lowered to the abort-tx cap by `bucketize_computation` when a
        /// Move abort is detected and the protocol config defines such a cap.
        ///
        /// The budget cap accounts for storage so the gas coin can always pay
        /// `net_gas_usage()` — if both computation and storage are attempted (e.g.
        /// storage OOG'd after a Move abort), the meter-derived computation is
        /// reduced to leave room for the accumulated net storage cost. Storage
        /// charges stick, computation absorbs whatever the budget can still afford.
        /// The bucketed, abort-discounted MIST cost of the computation accumulated
        /// in the Move VM meter so far. NO budget cap applied — callers that need
        /// the cap (`summary()`) should use `derived_computation_cost`; callers
        /// that need the "true" value to gate further charging (`bucketize_computation`,
        /// `charge_storage_and_rebate`) should use this one.
        ///
        /// The abort discount, if applicable, is folded in via
        /// `effective_gas_price` — `bucketize_computation` lowers that field when
        /// the result is a Move abort and the protocol config defines an abort cap.
        fn uncapped_computation_cost(&self) -> u64 {
            if self.force_computation_cost_to_budget {
                return self.gas_budget;
            }
            let raw_units = self.gas_status.gas_used_pre_gas_price();
            let bucketed_units = half_digits_rounding(raw_units);
            bucketed_units.saturating_mul(self.effective_gas_price)
        }

        /// Computation cost reported to the user via `summary()`. Caps so that
        /// `computation_cost + storage_cost - sender_rebate ≤ gas_budget` — when
        /// both computation and storage are attempted (e.g. storage OOG'd after
        /// computation succeeded), the meter-derived computation is reduced to
        /// leave room for the accumulated net storage cost. Storage charges
        /// stick, computation absorbs whatever the budget can still afford.
        fn derived_computation_cost(&self) -> u64 {
            let uncapped = self.uncapped_computation_cost();
            let storage_rebate = self.storage_rebate();
            let sender_rebate = sender_rebate(storage_rebate, self.rebate_rate);
            let net_storage = self.storage_cost().saturating_sub(sender_rebate);
            let max_computation = self.gas_budget.saturating_sub(net_storage);
            uncapped.min(max_computation)
        }

        // Gas data is consistent
        fn check_gas_data(
            &self,
            gas_objs: &[&ObjectReadResult],
            gas_budget: u64,
            available_address_balance_gas: u64,
        ) -> UserInputResult {
            // Gas budget is between min and max budget allowed
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

            // Gas balance (all gas coins + address balance together) is bigger or equal to budget
            let mut gas_balance = available_address_balance_gas as u128;
            for gas_obj in gas_objs {
                gas_balance += gas::get_gas_balance(gas_obj.as_object().ok_or(
                    UserInputError::InvalidGasObject {
                        object_id: gas_obj.id(),
                    },
                )?)? as u128;
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
            // Bucketize fixes the price `summary()` will use for the rest of the
            // pipeline — normally the user's `gas_price`, but lowered to the
            // abort-tx cap if this call reports a Move abort and the protocol
            // config defines such a cap. After that, it signals OOG if the
            // bucketed-and-priced computation alone exceeds the budget. The
            // bucketing math itself lives in `uncapped_computation_cost`, which
            // `summary()` calls on demand through `derived_computation_cost`.
            self.effective_gas_price = if aborted.unwrap_or(false) {
                let cap = self
                    .cost_table
                    .max_gas_price_rgp_factor_for_aborted_transactions
                    * self.reference_gas_price;
                self.user_gas_price.min(cap)
            } else {
                self.user_gas_price
            };
            if self.uncapped_computation_cost() >= self.gas_budget {
                return Err(ExecutionErrorKind::InsufficientGas.into());
            }
            Ok(())
        }

        /// Returns the gas cost summary derived directly from the Move VM meter state.
        /// `computation_cost = gas_used × effective_gas_price` (capped at
        /// `gas_budget − net_storage`). The effective price equals
        /// `user_gas_price` until `bucketize_computation` lowers it for a Move abort.
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

        /// Verify the accumulated `per_object_storage` fits in the remaining
        /// budget (`gas_budget − net_computation`). If the rebates already
        /// cover the cost, succeed unconditionally; otherwise compare net
        /// storage against the *uncapped* computation cost so the check
        /// reflects what the meter actually accumulated rather than the
        /// summary's capped report (the cap would otherwise always make
        /// storage appear to fit).
        fn charge_storage_and_rebate(&mut self) -> Result<(), ExecutionError> {
            let storage_rebate = self.storage_rebate();
            let storage_cost = self.storage_cost();
            let sender_rebate = sender_rebate(storage_rebate, self.rebate_rate);
            assert!(sender_rebate <= storage_rebate);
            if sender_rebate >= storage_cost {
                Ok(())
            } else {
                let gas_left = self
                    .gas_budget
                    .saturating_sub(self.uncapped_computation_cost());
                if gas_left < storage_cost - sender_rebate {
                    Err(ExecutionErrorKind::InsufficientGas.into())
                } else {
                    Ok(())
                }
            }
        }

        /// Force `summary()` to report `computation_cost == gas_budget` by
        /// dropping the accumulated `per_object_storage` and flipping
        /// `force_computation_cost_to_budget`. Setting the flag bypasses the
        /// meter math entirely; deriving from `gas_used_pre_gas_price() ×
        /// effective_gas_price` would otherwise lose up to `budget %
        /// user_gas_price` MIST. Callers: `set_computation_to_budget` on the
        /// err-path fallback when even input-only storage doesn't fit the
        /// budget, and the legacy storage-OOG handler.
        fn adjust_computation_on_out_of_gas(&mut self) {
            self.per_object_storage = Vec::new();
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
                storage_gas_price: self.storage_gas_price,
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
            self.check_gas_data(gas_objs, gas_budget, available_address_balance_gas)
        }

        // Check gas objects have an address owner.
        fn check_gas_objects(&self, gas_objs: &[&ObjectReadResult]) -> UserInputResult {
            // All gas objects have an address owner
            // Note: because of address balance payments, gas_objs may be empty.
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
            Ok(())
        }

        fn per_object_storage(&self) -> &Vec<(ObjectID, PerObjectStorage)> {
            &self.per_object_storage
        }
    }

    fn half_digits_rounding(n: u64) -> u64 {
        if n < 1000 {
            return 1000;
        }
        let digits = n.ilog10();
        let drop = digits / 2;
        let base = 10u64.pow(drop);
        n.div_ceil(base) * base
    }

    #[test]
    fn test_half_digits_rounding() {
        assert_eq!(half_digits_rounding(0), 1000);
        assert_eq!(half_digits_rounding(1), 1000);
        assert_eq!(half_digits_rounding(999), 1000);
        assert_eq!(half_digits_rounding(1000), 1000);
        assert_eq!(half_digits_rounding(1001), 1010);
        assert_eq!(half_digits_rounding(1050), 1050);
        assert_eq!(half_digits_rounding(1999), 2000);
        assert_eq!(half_digits_rounding(20_000), 20_000);
        assert_eq!(half_digits_rounding(20_001), 20_100);
        assert_eq!(half_digits_rounding(20_500), 20_500);
        assert_eq!(half_digits_rounding(29_999), 30_000);
        assert_eq!(half_digits_rounding(300_000), 300_000);
        assert_eq!(half_digits_rounding(300_001), 300_100);
        assert_eq!(half_digits_rounding(305_500), 305_500);
        assert_eq!(half_digits_rounding(305_501), 305_600);
        assert_eq!(half_digits_rounding(999_999), 1_000_000);
        assert_eq!(half_digits_rounding(1_000_000), 1_000_000);
        assert_eq!(half_digits_rounding(1_000_001), 1_001_000);
        assert_eq!(half_digits_rounding(1_005_000), 1_005_000);
        assert_eq!(half_digits_rounding(1_005_001), 1_006_000);
        assert_eq!(half_digits_rounding(1_999_999), 2_000_000);
        assert_eq!(half_digits_rounding(10_000_001), 10_001_000);
        assert_eq!(half_digits_rounding(100_000_001), 100_010_000);
    }
}
