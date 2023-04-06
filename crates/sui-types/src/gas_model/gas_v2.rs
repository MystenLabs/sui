// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{UserInputError, UserInputResult};
use crate::gas::{GasCostSummary, SuiGasStatusAPI};
use crate::{
    error::{ExecutionError, ExecutionErrorKind},
    gas_coin::GasCoin,
    object::{Object, Owner},
};
use move_core_types::vm_status::StatusCode;
use once_cell::sync::Lazy;
use std::convert::TryFrom;
use std::iter;
use sui_cost_tables::bytecode_tables::{GasStatus, INITIAL_COST_SCHEDULE};
use sui_protocol_config::*;

macro_rules! ok_or_gas_balance_error {
    ($balance:expr, $required:expr) => {
        if $balance < $required {
            Err(UserInputError::GasBalanceTooLow {
                gas_balance: $balance,
                needed_gas_amount: $required,
            })
        } else {
            Ok(())
        }
    };
}

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

pub(crate) fn get_bucket_cost(table: &[ComputationBucket], computation_cost: u64) -> u64 {
    for bucket in table {
        if bucket.max >= computation_cost {
            return bucket.cost;
        }
    }
    MAX_BUCKET_COST
}

// for a RPG of 1000 this amounts to about 5 SUI
pub(crate) const MAX_BUCKET_COST: u64 = 5_000_000;

// define the bucket table for computation charging
pub(crate) static COMPUTATION_BUCKETS: Lazy<Vec<ComputationBucket>> = Lazy::new(|| {
    vec![
        ComputationBucket::simple(0, 1_000),
        ComputationBucket::simple(1_000, 5_000),
        ComputationBucket::simple(5_000, 10_000),
        ComputationBucket::simple(10_000, 20_000),
        ComputationBucket::simple(20_000, 50_000),
        ComputationBucket::simple(50_000, 200_000),
        ComputationBucket::simple(200_000, 1_000_000),
        ComputationBucket::simple(1_000_000, MAX_BUCKET_COST),
    ]
});

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
}

impl std::fmt::Debug for SuiCostTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: dump the fields.
        write!(f, "SuiCostTable(...)")
    }
}

impl SuiCostTable {
    pub(crate) fn new(c: &ProtocolConfig) -> Self {
        Self {
            min_transaction_cost: c.base_tx_cost_fixed(),
            max_gas_budget: c.max_tx_gas(),
            package_publish_per_byte_cost: c.package_publish_cost_per_byte(),
            object_read_per_byte_cost: c.obj_access_cost_read_per_byte(),
            storage_per_byte_cost: c.obj_data_cost_refundable(),
        }
    }

    pub(crate) fn unmetered() -> Self {
        Self {
            min_transaction_cost: 0,
            max_gas_budget: u64::MAX,
            package_publish_per_byte_cost: 0,
            object_read_per_byte_cost: 0,
            storage_per_byte_cost: 0,
        }
    }
}

#[derive(Debug)]
pub struct SuiGasStatus<'a> {
    // GasStatus as used by the VM, that is all the VM sees
    gas_status: GasStatus<'a>,
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
    // Gas price for storage. This is a multiplier on the final charge
    // as related to the storage gas price defined in the system
    // (`ProtocolConfig::storage_gas_price`).
    // Conceptually, given a constant `obj_data_cost_refundable`
    // (defined in `ProtocolConfig::obj_data_cost_refundable`)
    // `total_storage_cost = storage_bytes * obj_data_cost_refundable`
    // `final_storage_cost = total_storage_cost * storage_gas_price`
    storage_gas_price: u64,
    /// storage_cost is the total storage gas to charge. This is an accumulator computed
    /// at the end of execution while determining storage charges.
    /// It tracks `total_storage_cost = storage_bytes * obj_data_cost_refundable` as
    /// described in `storage_gas_price`
    /// It will be multiplied by the storage gas price.
    storage_cost: u64,
    /// storage_rebate is the total storage rebate (in Sui) accumulated in this transaction.
    /// This is an accumulator computed at the end of execution while determining storage charges.
    /// It is the sum of all `storage_rebate` of all objects mutated or deleted during
    /// execution. The value is in Sui.
    storage_rebate: u64,
    // storage rebate rate as defined in the ProtocolConfig
    rebate_rate: u64,
    /// Amount of storage rebate accumulated when we are running in unmetered mode (i.e. system transaction).
    /// This allows us to track how much storage rebate we need to retain in system transactions.
    unmetered_storage_rebate: u64,
}

impl<'a> SuiGasStatus<'a> {
    fn new(
        move_gas_status: GasStatus<'a>,
        gas_budget: u64,
        charge: bool,
        gas_price: u64,
        storage_gas_price: u64,
        rebate_rate: u64,
        cost_table: SuiCostTable,
    ) -> SuiGasStatus<'a> {
        SuiGasStatus {
            gas_status: move_gas_status,
            gas_budget,
            charge,
            computation_cost: 0,
            gas_price,
            storage_gas_price,
            storage_cost: 0,
            storage_rebate: 0,
            rebate_rate,
            unmetered_storage_rebate: 0,
            cost_table,
        }
    }

    pub(crate) fn new_with_budget(
        gas_budget: u64,
        gas_price: u64,
        config: &ProtocolConfig,
    ) -> SuiGasStatus<'a> {
        let storage_gas_price = config.storage_gas_price();
        let max_computation_budget = MAX_BUCKET_COST * gas_price;
        let computation_budget = if gas_budget > max_computation_budget {
            max_computation_budget
        } else {
            gas_budget
        };
        Self::new(
            GasStatus::new_v2(&INITIAL_COST_SCHEDULE, computation_budget, gas_price),
            gas_budget,
            true,
            gas_price,
            storage_gas_price,
            config.storage_rebate_rate(),
            SuiCostTable::new(config),
        )
    }

    pub(crate) fn new_for_testing(
        gas_budget: u64,
        gas_price: u64,
        storage_gas_price: u64,
        cost_table: SuiCostTable,
    ) -> SuiGasStatus<'a> {
        let rebate_rate = ProtocolConfig::get_for_max_version().storage_rebate_rate();
        let max_computation_budget = MAX_BUCKET_COST;
        let computation_budget = if gas_budget > max_computation_budget {
            max_computation_budget
        } else {
            gas_budget
        };
        Self::new(
            GasStatus::new_v2(&INITIAL_COST_SCHEDULE, computation_budget, gas_price),
            gas_budget,
            true,
            gas_price,
            storage_gas_price,
            rebate_rate,
            cost_table,
        )
    }

    pub fn new_unmetered() -> SuiGasStatus<'a> {
        Self::new(
            GasStatus::new_unmetered(),
            0,
            false,
            0,
            0,
            0,
            SuiCostTable::unmetered(),
        )
    }
}

impl<'a> SuiGasStatusAPI<'a> for SuiGasStatus<'a> {
    fn is_unmetered(&self) -> bool {
        !self.charge
    }

    fn move_gas_status(&mut self) -> &mut GasStatus<'a> {
        &mut self.gas_status
    }

    fn bucketize_computation(&mut self) -> Result<(), ExecutionError> {
        let gas_used = self.gas_status.gas_used_pre_gas_price();
        let bucket_cost = get_bucket_cost(&COMPUTATION_BUCKETS, gas_used);
        // charge extra on top of `computation_cost` to make the total computation
        // cost a bucket value
        let gas_used = bucket_cost * self.gas_price;
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
        let sender_rebate = sender_rebate(self.storage_rebate, self.rebate_rate);
        assert!(sender_rebate <= self.storage_rebate);
        let non_refundable_storage_fee = self.storage_rebate - sender_rebate;
        GasCostSummary {
            computation_cost: self.computation_cost,
            storage_cost: self.storage_cost,
            storage_rebate: sender_rebate,
            non_refundable_storage_fee,
        }
    }

    fn gas_budget(&self) -> u64 {
        self.gas_budget
    }

    fn storage_gas_units(&self) -> u64 {
        self.storage_cost
    }

    fn storage_rebate(&self) -> u64 {
        self.storage_rebate
    }

    fn unmetered_storage_rebate(&self) -> u64 {
        self.unmetered_storage_rebate
    }

    fn gas_used(&self) -> u64 {
        self.gas_status.gas_used_pre_gas_price()
    }

    fn reset_storage_cost_and_rebate(&mut self) {
        self.storage_cost = 0;
        self.storage_rebate = 0;
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

    fn charge_storage_mutation(
        &mut self,
        _new_size: usize,
        _storage_rebate: u64,
    ) -> Result<u64, ExecutionError> {
        Err(ExecutionError::invariant_violation(
            "charge_storage_mutation should not be called in v2 gas model",
        ))
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
    fn track_storage_mutation(&mut self, new_size: usize, storage_rebate: u64) -> u64 {
        if self.is_unmetered() {
            self.unmetered_storage_rebate += storage_rebate;
            return 0;
        }
        self.storage_rebate += storage_rebate;
        // compute and track cost (based on size)
        let new_size = new_size as u64;
        let storage_cost =
            new_size * self.cost_table.storage_per_byte_cost * self.storage_gas_price;
        // track rebate
        self.storage_cost += storage_cost;
        // return the new object rebate (object storage cost)
        storage_cost
    }

    fn charge_storage_and_rebate(&mut self) -> Result<(), ExecutionError> {
        let sender_rebate = sender_rebate(self.storage_rebate, self.rebate_rate);
        assert!(sender_rebate <= self.storage_rebate);
        if sender_rebate >= self.storage_cost {
            // there is more rebate than cost, when deducting gas we are adding
            // to whatever is the current amount charged so we are `Ok`
            Ok(())
        } else {
            let gas_left = self.gas_budget - self.computation_cost;
            // we have to charge for storage and may go out of gas, check
            if gas_left < self.storage_cost - sender_rebate {
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
        self.storage_rebate = 0;
        self.storage_cost = 0;
        self.computation_cost = self.gas_budget;
    }
}

// Check whether gas arguments are legit:
// 1. Gas object has an address owner.
// 2. Gas budget is between min and max budget allowed
// 3. Gas balance (all gas coins together) is bigger or equal to budget
pub(crate) fn check_gas_balance(
    gas_object: &Object,
    more_gas_objs: Vec<&Object>,
    gas_budget: u64,
    cost_table: &SuiCostTable,
) -> UserInputResult {
    // 1. All gas objects have an address owner
    for gas_object in more_gas_objs.iter().chain(iter::once(&gas_object)) {
        if !(matches!(gas_object.owner, Owner::AddressOwner(_))) {
            return Err(UserInputError::GasObjectNotOwnedObject {
                owner: gas_object.owner,
            });
        }
    }

    // 2. Gas budget is between min and max budget allowed
    if gas_budget > cost_table.max_gas_budget {
        return Err(UserInputError::GasBudgetTooHigh {
            gas_budget,
            max_budget: cost_table.max_gas_budget,
        });
    }
    if gas_budget < cost_table.min_transaction_cost {
        return Err(UserInputError::GasBudgetTooLow {
            gas_budget,
            min_budget: cost_table.min_transaction_cost,
        });
    }

    // 3. Gas balance (all gas coins together) is bigger or equal to budget
    let mut gas_balance = get_gas_balance(gas_object)? as u128;
    for extra_obj in more_gas_objs {
        gas_balance += get_gas_balance(extra_obj)? as u128;
    }
    ok_or_gas_balance_error!(gas_balance, gas_budget as u128)
}

/// Subtract the gas balance of \p gas_object by \p amount.
/// This function should never fail, since we checked that the budget is always
/// less than balance, and the amount is capped at the budget.
pub fn deduct_gas(gas_object: &mut Object, charge_or_rebate: i64) {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    let new_balance = if charge_or_rebate < 0 {
        balance + (-charge_or_rebate as u64)
    } else {
        assert!(balance >= charge_or_rebate as u64);
        balance - charge_or_rebate as u64
    };
    let new_gas_coin = GasCoin::new(*gas_coin.id(), new_balance);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    // unwrap safe because GasCoin is guaranteed to serialize
    let new_contents = bcs::to_bytes(&new_gas_coin).unwrap();
    assert_eq!(move_object.contents().len(), new_contents.len());
    move_object.update_coin_contents(new_contents);
}

pub fn get_gas_balance(gas_object: &Object) -> UserInputResult<u64> {
    Ok(GasCoin::try_from(gas_object)
        .map_err(|_e| UserInputError::InvalidGasObject {
            object_id: gas_object.id(),
        })?
        .value())
}
