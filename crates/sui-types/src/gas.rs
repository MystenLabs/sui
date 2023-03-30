// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{UserInputError, UserInputResult};
use crate::messages::TransactionEffects;
use crate::{
    error::{ExecutionError, ExecutionErrorKind},
    gas_coin::GasCoin,
    messages::TransactionEffectsAPI,
    object::{Object, Owner},
};
use itertools::MultiUnzip;
use move_core_types::{
    gas_algebra::{GasQuantity, InternalGas, InternalGasPerByte, NumBytes, UnitDiv},
    vm_status::StatusCode,
};
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::AddAssign;
use std::{
    convert::TryFrom,
    ops::{Add, Deref, Mul},
};
use sui_cost_tables::{
    bytecode_tables::{GasStatus, INITIAL_COST_SCHEDULE},
    units_types::GasUnit,
};
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

sui_macros::checked_arithmetic! {

// A bucket defines a range of units that will be priced the same.
// A cost for the bucket is defined to make the step function non linear.
#[allow(dead_code)]
struct ComputationBucket {
    min: u64,
    max: u64,
    cost: u64,
}

impl ComputationBucket {
    fn new(min: u64, max: u64, cost: u64) -> Self {
        ComputationBucket { min, max, cost }
    }

    fn simple(min: u64, max: u64) -> Self {
        ComputationBucket {
            min,
            max,
            cost: max,
        }
    }
}

fn get_bucket_cost(table: &[ComputationBucket], computation_cost: u64) -> u64 {
    for bucket in table {
        if bucket.max >= computation_cost {
            return bucket.cost;
        }
    }
    MAX_BUCKET_COST
}

// for a RPG of 1000 this amounts to about 1 SUI
const MAX_BUCKET_COST: u64 = 1_000_000;

// define the bucket table for computation charging
static COMPUTATION_BUCKETS: Lazy<Vec<ComputationBucket>> = Lazy::new(|| {
    vec![
        ComputationBucket::simple(0, 1_000),
        ComputationBucket::simple(1_001, 5_000),
        ComputationBucket::simple(5_001, 10_000),
        ComputationBucket::simple(10_001, 20_000),
        ComputationBucket::simple(20_001, 50_000),
        ComputationBucket::new(50_001, u64::MAX, MAX_BUCKET_COST),
    ]
});

pub type GasUnits = GasQuantity<GasUnit>;
pub enum GasPriceUnit {}
pub enum SuiGasUnit {}

pub type ComputeGasPricePerUnit = GasQuantity<UnitDiv<GasUnit, GasUnit>>;

pub type GasPrice = GasQuantity<GasPriceUnit>;
pub type SuiGas = GasQuantity<SuiGasUnit>;

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
#[derive(Eq, PartialEq, Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GasCostSummary {
    /// Cost of computation/execution
    pub computation_cost: u64,
    /// Storage cost, it's the sum of all storage cost for all objects created or mutated.
    pub storage_cost: u64,
    /// The amount of storage cost refunded to the user for all objects deleted or mutated in the
    /// transaction.
    pub storage_rebate: u64,
    /// The fee for the rebate. The portion of the storage rebate kept by the system.
    pub non_refundable_storage_fee: u64,
}

impl GasCostSummary {
    pub fn new(computation_cost: u64, storage_cost: u64, storage_rebate: u64) -> GasCostSummary {
        GasCostSummary {
            computation_cost,
            storage_cost,
            storage_rebate,
            non_refundable_storage_fee: 0,
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

    /// Portion of the storage rebate that flows back into the storage fund.
    /// This will be burned, then re-minted + added to the storage fund at the next epoch change.
    /// Note: these funds are not accounted for in the `storage_refund` field of any Sui object
    pub fn storage_fund_rebate_inflow(&self, storage_rebate_rate: u64) -> u64 {
        self.storage_rebate - self.sender_rebate(storage_rebate_rate)
    }

    /// Get net gas usage, positive number means used gas; negative number means refund.
    pub fn net_gas_usage(&self) -> i64 {
        self.gas_used() as i64 - self.storage_rebate as i64
    }

    pub fn new_from_txn_effects<'a>(
        transactions: impl Iterator<Item = &'a TransactionEffects>,
    ) -> GasCostSummary {
        let (storage_costs, computation_costs, storage_rebates): (Vec<u64>, Vec<u64>, Vec<u64>) =
            transactions
                .map(|e| {
                    (
                        e.gas_cost_summary().storage_cost,
                        e.gas_cost_summary().computation_cost,
                        e.gas_cost_summary().storage_rebate,
                    )
                })
                .multiunzip();

        GasCostSummary {
            storage_cost: storage_costs.iter().sum(),
            computation_cost: computation_costs.iter().sum(),
            storage_rebate: storage_rebates.iter().sum(),
            non_refundable_storage_fee: 0,
        }
    }
}

impl std::fmt::Display for GasCostSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "computation_cost: {}, storage_cost: {},  storage_rebate: {}",
            self.computation_cost, self.storage_cost, self.storage_rebate
        )
    }
}

// Fixed cost type
#[derive(Clone)]
pub struct FixedCost(InternalGas);
impl FixedCost {
    pub fn new(x: u64) -> Self {
        FixedCost(InternalGas::new(x))
    }
}
impl Deref for FixedCost {
    type Target = InternalGas;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// ComputationCostPerByte is a newtype wrapper of InternalGas
/// to ensure a value of this type is used specifically for computation cost.
/// Anything that does not change the amount of bytes stored in the authority data store
/// will charge ComputationCostPerByte.
pub struct ComputationCostPerByte(InternalGasPerByte);

impl ComputationCostPerByte {
    pub fn new(x: u64) -> Self {
        ComputationCostPerByte(InternalGasPerByte::new(x))
    }
}

impl Deref for ComputationCostPerByte {
    type Target = InternalGasPerByte;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// StorageCostPerByte is a newtype wrapper of InternalGas
/// to ensure a value of this type is used specifically for storage cost.
/// Anything that changes the amount of bytes stored in the authority data store
/// will charge StorageCostPerByte.
pub struct StorageCostPerByte(InternalGasPerByte);

impl Deref for StorageCostPerByte {
    type Target = InternalGasPerByte;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl StorageCostPerByte {
    pub fn new(x: u64) -> Self {
        StorageCostPerByte(InternalGasPerByte::new(x))
    }
}

/// A list of constant costs of various operations in Sui.
pub struct SuiCostTable {
    /// A flat fee charged for every transaction. This is also the mimmum amount of
    /// gas charged for a transaction.
    pub min_transaction_cost: FixedCost,
    /// Maximum allowable budget for a transaction.
    pub max_gas_budget: u64,
    /// Computation cost per byte charged for package publish. This cost is primarily
    /// determined by the cost to verify and link a package. Note that this does not
    /// include the cost of writing the package to the store.
    pub package_publish_per_byte_cost: ComputationCostPerByte,
    /// Per byte cost to read objects from the store. This is computation cost instead of
    /// storage cost because it does not change the amount of data stored on the db.
    pub object_read_per_byte_cost: ComputationCostPerByte,
    /// Per byte cost to write objects to the store. This is computation cost instead of
    /// storage cost because it does not change the amount of data stored on the db.
    pub object_mutation_per_byte_cost: ComputationCostPerByte,
    /// Unit cost of a byte in the storage. This will be used both for charging for
    /// new storage as well as rebating for deleting storage. That is, we expect users to
    /// get full refund on the object storage when it's deleted.
    pub storage_per_byte_cost: StorageCostPerByte,
}

impl std::fmt::Debug for SuiCostTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: dump the fields.
        write!(f, "SuiCostTable(...)")
    }
}

impl SuiCostTable {
    pub fn new(c: &ProtocolConfig) -> Self {
        Self {
            min_transaction_cost: FixedCost::new(c.base_tx_cost_fixed()),
            max_gas_budget: c.max_tx_gas(),
            package_publish_per_byte_cost: ComputationCostPerByte::new(
                c.package_publish_cost_per_byte(),
            ),
            object_read_per_byte_cost: ComputationCostPerByte::new(
                c.obj_access_cost_read_per_byte(),
            ),
            object_mutation_per_byte_cost: ComputationCostPerByte::new(
                c.obj_access_cost_mutate_per_byte(),
            ),
            storage_per_byte_cost: StorageCostPerByte::new(c.obj_data_cost_refundable()),
        }
    }

    pub fn new_for_testing() -> Self {
        Self::new(&ProtocolConfig::get_for_max_version())
    }

    fn unmetered() -> Self {
        Self {
            min_transaction_cost: FixedCost::new(0),
            max_gas_budget: u64::MAX,
            package_publish_per_byte_cost: ComputationCostPerByte::new(0),
            object_read_per_byte_cost: ComputationCostPerByte::new(0),
            object_mutation_per_byte_cost: ComputationCostPerByte::new(0),
            storage_per_byte_cost: StorageCostPerByte::new(0),
        }
    }

    pub fn min_gas_budget_external(&self) -> u64 {
        u64::from(to_external(*self.min_transaction_cost))
    }
}

fn to_external(internal_units: InternalGas) -> GasUnits {
    InternalGas::to_unit_round_down(internal_units)
}

#[derive(Debug)]
pub struct SuiGasStatus<'a> {
    gas_status: GasStatus<'a>,
    init_budget: GasUnits,
    charge: bool,
    computation_gas_unit_price: ComputeGasPricePerUnit,
    storage_gas_unit_price: ComputeGasPricePerUnit,
    /// storage_cost is the total storage gas units charged so far, due to writes into storage.
    /// It will be multiplied by the storage gas unit price in the end to obtain the Sui cost.
    storage_gas_units: GasUnits,
    /// storage_rebate is the total storage rebate (in Sui) accumulated in this transaction.
    /// It's directly coming from each mutated object's storage rebate field, which
    /// was the storage cost paid when the object was last mutated. It is not affected
    /// by the current storage gas unit price.
    storage_rebate: SuiGas,
    /// Amount of storage rebate accumulated when we are running in unmetered mode (i.e. system transaction).
    /// This allows us to track how much storage rebate we need to retain in system transactions.
    unmetered_storage_rebate: SuiGas,

    cost_table: SuiCostTable,
}

fn to_internal(external_units: GasUnits) -> InternalGas {
    GasUnits::to_unit(external_units)
}

impl<'a> SuiGasStatus<'a> {
    pub fn new_with_budget(
        gas_budget: u64,
        computation_gas_unit_price: GasPrice,
        storage_gas_unit_price: GasPrice,
        cost_table: SuiCostTable,
    ) -> SuiGasStatus<'a> {
        let gas_price: u64 = computation_gas_unit_price.into();
        let budget_in_unit = gas_budget / gas_price; // truncate the value and move to units
        Self::new(
            GasStatus::new(&INITIAL_COST_SCHEDULE, GasUnits::new(budget_in_unit)),
            budget_in_unit,
            true,
            computation_gas_unit_price,
            storage_gas_unit_price.into(),
            cost_table,
        )
    }

    pub fn new_unmetered() -> SuiGasStatus<'a> {
        Self::new(
            GasStatus::new_unmetered(),
            0,
            false,
            0.into(),
            0,
            SuiCostTable::unmetered(),
        )
    }

    pub fn is_unmetered(&self) -> bool {
        !self.charge
    }

    pub fn computation_gas_remaining(&self) -> u64 {
        self.gas_status.remaining_gas().into()
    }

    pub fn storage_rebate(&self) -> u64 {
        self.storage_rebate.into()
    }

    pub fn unmetered_storage_rebate(&self) -> u64 {
        self.unmetered_storage_rebate.into()
    }

    pub fn storage_gas_units(&self) -> u64 {
        self.storage_gas_units.into()
    }

    pub fn max_gax_budget_in_balance(&self) -> u64 {
        // MUSTFIX: Properly compute gas budget
        let max_gas_unit_price =
            std::cmp::max(self.computation_gas_unit_price, self.storage_gas_unit_price);
        self.init_budget.mul(max_gas_unit_price).into()
    }

    pub fn create_move_gas_status(&mut self) -> &mut GasStatus<'a> {
        &mut self.gas_status
    }

    pub fn bucketize_computation(&mut self) -> Result<(), ExecutionError> {
        let computation_cost: u64 = self.gas_used().into();
        let bucket_cost = get_bucket_cost(&COMPUTATION_BUCKETS, computation_cost);
        // charge extra on top of `computation_cost` to make the total computation
        // gas cost a bucket value
        let extra_charge = bucket_cost.saturating_sub(computation_cost);
        if extra_charge > 0 {
            self.deduct_computation_cost(&GasUnits::new(extra_charge).to_unit())
        } else {
            // we hit the last bucket and the computation is already more then the
            // max bucket so just charge as much as it is without buckets
            Ok(())
        }
    }

    pub fn reset_storage_cost_and_rebate(&mut self) {
        self.storage_gas_units = GasQuantity::zero();
        self.storage_rebate = GasQuantity::zero();
        self.unmetered_storage_rebate = GasQuantity::zero();
    }

    /// Try to charge the minimal amount of gas from the gas object.
    /// This function is called in tx signing phase to make sure
    /// the gas object has enough balance to cover minimal transaction cost.
    pub fn charge_min_tx_gas(&mut self) -> UserInputResult {
        let cost = self.cost_table.min_transaction_cost.clone();
        self.deduct_computation_cost(cost.deref())
            .map_err(|_e| UserInputError::InsufficientBalanceToCoverMinimalGas)
    }

    pub fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError> {
        let computation_cost =
            NumBytes::new(size as u64).mul(*self.cost_table.package_publish_per_byte_cost);

        self.deduct_computation_cost(&computation_cost)
    }

    pub fn charge_storage_read(&mut self, size: usize) -> Result<(), ExecutionError> {
        let cost = NumBytes::new(size as u64).mul(*self.cost_table.object_read_per_byte_cost);
        self.deduct_computation_cost(&cost)
    }

    pub fn charge_computation_gas_for_storage_mutation(
        &mut self,
        size: u64,
    ) -> Result<(), ExecutionError> {
        let cost = NumBytes::new(size).mul(*self.cost_table.object_mutation_per_byte_cost);
        self.deduct_computation_cost(&cost)
    }

    pub fn charge_storage_mutation(
        &mut self,
        new_size: usize,
        storage_rebate: SuiGas,
    ) -> Result<u64, ExecutionError> {
        if self.is_unmetered() {
            self.unmetered_storage_rebate.add_assign(storage_rebate);
            return Ok(0);
        }

        let storage_cost =
            NumBytes::new(new_size as u64).mul(*self.cost_table.storage_per_byte_cost);
        self.deduct_storage_cost(&storage_cost).map(|gu| {
            self.storage_rebate.add_assign(storage_rebate);
            gu.into()
        })
    }

    /// This function is only called during testing, where we need to mock
    /// Move VM charging gas.
    pub fn charge_vm_exec_test_only(&mut self, cost: u64) -> Result<(), ExecutionError> {
        self.gas_status
            .deduct_gas(InternalGas::new(cost))
            .map_err(|e| {
                debug_assert_eq!(e.major_status(), StatusCode::OUT_OF_GAS);
                ExecutionErrorKind::InsufficientGas.into()
            })
    }

    /// Returns the final (computation cost, storage cost, storage rebate) of the gas meter.
    /// We use initial budget, combined with remaining gas and storage cost to derive
    /// computation cost.
    pub fn summary(&self) -> GasCostSummary {
        let remaining_gas = self.gas_status.remaining_gas();
        let storage_cost = self.storage_gas_units;
        // MUSTFIX: handle underflow how?
        let computation_cost = self
            .init_budget
            .checked_sub(remaining_gas)
            .expect("Subtraction overflowed")
            .checked_sub(storage_cost)
            .expect("Subtraction overflowed");

        let computation_cost_in_sui = computation_cost.mul(self.computation_gas_unit_price).into();
        GasCostSummary {
            computation_cost: computation_cost_in_sui,
            storage_cost: storage_cost.mul(self.storage_gas_unit_price).into(),
            storage_rebate: self.storage_rebate.into(),
            non_refundable_storage_fee: 0,
        }
    }

    fn new(
        move_gas_status: GasStatus<'a>,
        gas_budget: u64,
        charge: bool,
        computation_gas_unit_price: GasPrice,
        storage_gas_unit_price: u64,
        cost_table: SuiCostTable,
    ) -> SuiGasStatus<'a> {
        SuiGasStatus {
            gas_status: move_gas_status,
            init_budget: GasUnits::new(gas_budget),
            charge,
            computation_gas_unit_price: ComputeGasPricePerUnit::new(
                computation_gas_unit_price.into(),
            ),
            storage_gas_unit_price: ComputeGasPricePerUnit::new(storage_gas_unit_price),
            storage_gas_units: GasUnits::new(0),
            storage_rebate: 0.into(),
            unmetered_storage_rebate: 0.into(),
            cost_table,
        }
    }

    fn deduct_computation_cost(&mut self, cost: &InternalGas) -> Result<(), ExecutionError> {
        self.gas_status.deduct_gas(*cost).map_err(|e| {
            debug_assert_eq!(e.major_status(), StatusCode::OUT_OF_GAS);
            ExecutionErrorKind::InsufficientGas.into()
        })
    }

    fn deduct_storage_cost(&mut self, cost: &InternalGas) -> Result<GasUnits, ExecutionError> {
        if self.is_unmetered() {
            return Ok(0.into());
        }
        let ext_cost = to_external(NumBytes::new(1).mul(InternalGasPerByte::new(u64::from(*cost))));
        let charge_amount = to_internal(ext_cost);
        let remaining_gas = self.gas_status.remaining_gas();
        if self.gas_status.deduct_gas(charge_amount).is_err() {
            debug_assert_eq!(u64::from(self.gas_status.remaining_gas()), 0);
            // Even when we run out of gas, we still keep track of the storage_cost change,
            // so that at the end, we could still use it to accurately derive the
            // computation cost.
            self.storage_gas_units = self.storage_gas_units.add(remaining_gas);
            Err(ExecutionErrorKind::InsufficientGas.into())
        } else {
            self.storage_gas_units = self.storage_gas_units.add(ext_cost);
            Ok(ext_cost.mul(self.storage_gas_unit_price))
        }
    }

    pub fn gas_used(&self) -> GasUnits {
        let remaining_gas = self.gas_status.remaining_gas();
        self.init_budget
            .checked_sub(remaining_gas)
            .expect("Subtraction overflowed")
    }
}

// Check whether gas arguments are legit:
// 1. Gas object has an address owner.
// 2. Gas budget is between min and max budget allowed
pub fn check_gas_balance(
    gas_object: &Object,
    more_gas_objs: Vec<&Object>,
    gas_budget: u64,
    gas_price: u64,
    cost_table: &SuiCostTable,
) -> UserInputResult {
    // 1. Gas object has an address owner.
    if !(matches!(gas_object.owner, Owner::AddressOwner(_))) {
        return Err(UserInputError::GasObjectNotOwnedObject {
            owner: gas_object.owner,
        });
    }

    // 2. Gas budget is between min and max budget allowed
    let max_gas_budget = cost_table.max_gas_budget as u128 * gas_price as u128;
    let min_gas_budget = cost_table.min_gas_budget_external() as u128 * gas_price as u128;
    let required_gas_amount = gas_budget as u128;
    if required_gas_amount > max_gas_budget {
        return Err(UserInputError::GasBudgetTooHigh {
            gas_budget,
            max_budget: cost_table.max_gas_budget,
        });
    }
    if required_gas_amount < min_gas_budget {
        return Err(UserInputError::GasBudgetTooLow {
            gas_budget,
            min_budget: cost_table.min_gas_budget_external(),
        });
    }

    // 3. Gas balance (all gas coins together) is bigger or equal to budget
    let mut gas_balance = get_gas_balance(gas_object)? as u128;
    for extra_obj in more_gas_objs {
        gas_balance += get_gas_balance(extra_obj)? as u128;
    }
    ok_or_gas_balance_error!(gas_balance, required_gas_amount)
}

/// Create a new gas status with the given `gas_budget`, and charge the transaction flat fee.
pub fn start_gas_metering(
    gas_budget: u64,
    computation_gas_unit_price: u64,
    storage_gas_unit_price: u64,
    cost_table: SuiCostTable,
) -> UserInputResult<SuiGasStatus<'static>> {
    Ok(SuiGasStatus::new_with_budget(
        gas_budget,
        computation_gas_unit_price.into(),
        storage_gas_unit_price.into(),
        cost_table,
    ))
}

/// Subtract the gas balance of \p gas_object by \p amount.
/// This function should never fail, since we checked that the budget is always
/// less than balance, and the amount is capped at the budget.
pub fn deduct_gas(gas_object: &mut Object, deduct_amount: u64, rebate_amount: u64) {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    assert!(balance >= deduct_amount);
    let new_gas_coin = GasCoin::new(*gas_coin.id(), balance + rebate_amount - deduct_amount);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    // unwrap safe because GasCoin is guaranteed to serialize
    let new_contents = bcs::to_bytes(&new_gas_coin).unwrap();
    assert_eq!(move_object.contents().len(), new_contents.len());
    move_object.update_coin_contents(new_contents);
}

pub fn refund_gas(gas_object: &mut Object, amount: u64) {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    let new_gas_coin = GasCoin::new(*gas_coin.id(), balance + amount);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    // unwrap safe because GasCoin is guaranteed to serialize
    let new_contents = bcs::to_bytes(&new_gas_coin).unwrap();
    // unwrap because safe gas object cannot exceed max object size
    move_object.update_coin_contents(new_contents);
}

pub fn get_gas_balance(gas_object: &Object) -> UserInputResult<u64> {
    Ok(GasCoin::try_from(gas_object)
        .map_err(|_e| UserInputError::InvalidGasObject {
            object_id: gas_object.id(),
        })?
        .value())
}

}
