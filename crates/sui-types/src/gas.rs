// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;
use serde::{Deserialize, Serialize};

use crate::{base_types::ObjectID, gas_model::gas_v2::PerObjectStorage};

#[sui_macros::with_checked_arithmetic]
pub mod checked {

    use crate::gas::GasUsageReport;
    use crate::gas_model::gas_common::{check_gas_data, check_gas_objects};
    use crate::gas_model::gas_predicates::{
        check_for_gas_price_too_high, txn_base_cost_as_multiplier,
    };
    use crate::gas_model::gas_v2::PerObjectStorage;
    use crate::{
        ObjectID,
        effects::{TransactionEffects, TransactionEffectsAPI},
        error::{ExecutionError, UserInputError, UserInputResult},
        gas_model::{
            gas_v2::SuiGasStatus as SuiGasStatusV2, gas_v3::SuiGasStatus as SuiGasStatusV3,
            tables::GasStatus,
        },
        object::Object,
        sui_serde::{BigInt, Readable},
        transaction::{GasData, ObjectReadResult, TransactionKind, is_gasless_transaction},
    };
    use enum_dispatch::enum_dispatch;
    use itertools::MultiUnzip;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_with::serde_as;
    use sui_protocol_config::ProtocolConfig;

    #[enum_dispatch]
    pub trait SuiGasStatusAPI {
        fn is_unmetered(&self) -> bool;
        fn move_gas_status(&self) -> &GasStatus;
        fn move_gas_status_mut(&mut self) -> &mut GasStatus;
        fn bucketize_computation(&mut self, aborted: Option<bool>) -> Result<(), ExecutionError>;
        fn summary(&self) -> GasCostSummary;
        fn gas_budget(&self) -> u64;
        fn gas_price(&self) -> u64;
        fn reference_gas_price(&self) -> u64;
        fn storage_gas_units(&self) -> u64;
        fn storage_rebate(&self) -> u64;
        fn unmetered_storage_rebate(&self) -> u64;
        fn gas_used(&self) -> u64;
        fn reset_storage_cost_and_rebate(&mut self);
        fn charge_storage_read(&mut self, size: usize) -> Result<(), ExecutionError>;
        fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError>;
        fn track_storage_mutation(
            &mut self,
            object_id: ObjectID,
            new_size: usize,
            storage_rebate: u64,
        ) -> Option<u64>;
        fn charge_storage_and_rebate(&mut self) -> Result<(), ExecutionError>;
        fn adjust_computation_on_out_of_gas(&mut self);
        fn gas_usage_report(&self) -> GasUsageReport;
        fn per_object_storage(&self) -> &Vec<(ObjectID, PerObjectStorage)>;
    }

    /// Version-aware gas status: `V2` is the legacy model (`gas_model_version < 15`, incl. replay),
    /// `V3` is the v15+ pipeline. Dispatched by `gas_model_version() >= 15`.
    #[enum_dispatch(SuiGasStatusAPI)]
    #[derive(Debug)]
    pub enum SuiGasStatus {
        // V1 does not exists any longer as it was a pre mainnet version.
        // So we start the enum from V2
        V2(SuiGasStatusV2),
        V3(SuiGasStatusV3),
    }

    impl SuiGasStatus {
        /// Charge reading a package input. Earlier gas models charge packages as other inputs.
        pub fn charge_package_object_read(&mut self, size: usize) -> Result<(), ExecutionError> {
            match self {
                Self::V2(status) => status.charge_storage_read(size),
                Self::V3(status) => status.charge_package_object_read(size),
            }
        }

        /// Build the status for execution. Validation lives in `check_gas_price` and
        /// `check_gas_budget_and_balance`.
        pub fn new(
            gas_budget: u64,
            gas_price: u64,
            reference_gas_price: u64,
            config: &ProtocolConfig,
        ) -> Self {
            // Dispatch by gas model version: v15+ uses the clean gas_v3 pipeline;
            // everything older keeps the legacy gas_v2 path so replay determinism holds.
            if config.gas_model_version() >= 15 {
                Self::V3(SuiGasStatusV3::new_with_budget(
                    gas_budget,
                    gas_price,
                    reference_gas_price,
                    config,
                ))
            } else {
                Self::V2(SuiGasStatusV2::new_with_budget(
                    gas_budget,
                    gas_price,
                    reference_gas_price,
                    config,
                ))
            }
        }

        /// The status a transaction is executed with, derived from what signing validated.
        pub fn for_execution(
            gas_data: &GasData,
            kind: &TransactionKind,
            reference_gas_price: u64,
            config: &ProtocolConfig,
        ) -> Self {
            match execution_gas_params(gas_data, kind, reference_gas_price, config) {
                None => Self::new_unmetered(config),
                Some((budget, price)) => Self::new(budget, price, reference_gas_price, config),
            }
        }

        pub fn new_unmetered(config: &ProtocolConfig) -> Self {
            // Same dispatch as `new`: v15+ uses gas_v3, everything older keeps gas_v2 so
            // replay determinism holds.
            if config.gas_model_version() >= 15 {
                Self::V3(SuiGasStatusV3::new_unmetered())
            } else {
                Self::V2(SuiGasStatusV2::new_unmetered())
            }
        }
    }

    /// Budget and price a transaction is metered with, or `None` when it runs unmetered. Gasless
    /// transactions are metered at the reference price up to the computation cap.
    pub fn execution_gas_params(
        gas_data: &GasData,
        kind: &TransactionKind,
        reference_gas_price: u64,
        config: &ProtocolConfig,
    ) -> Option<(u64, u64)> {
        if gas_data.is_unmetered() || kind.is_system_tx() {
            return None;
        }
        if config.enable_gasless() && is_gasless_transaction(gas_data, kind) {
            let price = reference_gas_price.max(1);
            let budget = config.gasless_max_computation_units().saturating_mul(price);
            return Some((budget, price));
        }
        Some((gas_data.budget, gas_data.price))
    }

    /// Signing-time check of the gas price against the reference price and the maximum.
    pub fn check_gas_price(
        gas_price: u64,
        reference_gas_price: u64,
        config: &ProtocolConfig,
    ) -> UserInputResult {
        if gas_price < reference_gas_price {
            return Err(UserInputError::GasPriceUnderRGP {
                gas_price,
                reference_gas_price,
            });
        }
        if check_for_gas_price_too_high(config.gas_model_version())
            && gas_price >= config.max_gas_price()
        {
            return Err(UserInputError::GasPriceTooHigh {
                max_gas_price: config.max_gas_price(),
            });
        }
        Ok(())
    }

    /// The smallest budget a transaction may sign for: the base transaction cost at the signed
    /// price, a flat amount under the oldest gas models.
    pub fn minimum_gas_budget(config: &ProtocolConfig, gas_price: u64) -> u64 {
        let base_tx_cost = config.base_tx_cost_fixed();
        if config.gas_model_version() >= 15 || txn_base_cost_as_multiplier(config) {
            base_tx_cost
                .checked_mul(gas_price)
                .expect("base tx cost cannot overflow: gas_price is bounded by max_gas_price")
        } else {
            base_tx_cost
        }
    }

    /// Signing-time check that the gas objects are address owned and that the budget is within
    /// bounds and covered by them plus the address balance.
    pub fn check_gas_budget_and_balance(
        gas_objs: &[&ObjectReadResult],
        gas_budget: u64,
        gas_price: u64,
        available_address_balance_gas: u64,
        config: &ProtocolConfig,
    ) -> UserInputResult {
        check_gas_objects(gas_objs)?;
        check_gas_data(
            gas_objs,
            gas_budget,
            available_address_balance_gas,
            minimum_gas_budget(config, gas_price),
            config.max_tx_gas(),
        )
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
                self.computation_cost,
                self.storage_cost,
                self.storage_rebate,
                self.non_refundable_storage_fee,
            )
        }
    }

    impl std::ops::AddAssign<&Self> for GasCostSummary {
        fn add_assign(&mut self, other: &Self) {
            self.computation_cost += other.computation_cost;
            self.storage_cost += other.storage_cost;
            self.storage_rebate += other.storage_rebate;
            self.non_refundable_storage_fee += other.non_refundable_storage_fee;
        }
    }

    impl std::ops::AddAssign<Self> for GasCostSummary {
        fn add_assign(&mut self, other: Self) {
            self.add_assign(&other)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasUsageReport {
    pub cost_summary: GasCostSummary,
    pub gas_used: u64,
    pub gas_budget: u64,
    pub gas_price: u64,
    pub reference_gas_price: u64,
    pub storage_gas_price: u64,
    pub rebate_rate: u64,
    pub per_object_storage: Vec<(ObjectID, PerObjectStorage)>,
}
