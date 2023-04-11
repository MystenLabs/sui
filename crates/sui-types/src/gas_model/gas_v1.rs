// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{UserInputError, UserInputResult};
use crate::{
    error::{ExecutionError, ExecutionErrorKind},
    gas::{get_gas_balance, GasCostSummary, SuiGasStatusAPI},
    object::{Object, Owner},
};
use move_core_types::{
    gas_algebra::{GasQuantity, InternalGas, InternalGasPerByte, NumBytes, UnitDiv},
    vm_status::StatusCode,
};
use once_cell::sync::Lazy;
use std::ops::AddAssign;
use std::ops::{Add, Deref, Mul};
use sui_cost_tables::bytecode_tables::{INITIAL_COST_SCHEDULE, ZERO_COST_SCHEDULE};
use sui_cost_tables::units_types::CostTable;
use sui_cost_tables::{bytecode_tables::GasStatus, units_types::GasUnit};
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

type GasUnits = GasQuantity<GasUnit>;
enum GasPriceUnit {}
enum SuiGasUnit {}

type ComputeGasPricePerUnit = GasQuantity<UnitDiv<GasUnit, GasUnit>>;

type GasPrice = GasQuantity<GasPriceUnit>;
type SuiGas = GasQuantity<SuiGasUnit>;

// Fixed cost type
#[derive(Clone)]
struct FixedCost(InternalGas);
impl FixedCost {
    fn new(x: u64) -> Self {
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
struct ComputationCostPerByte(InternalGasPerByte);

impl ComputationCostPerByte {
    fn new(x: u64) -> Self {
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
struct StorageCostPerByte(InternalGasPerByte);

impl Deref for StorageCostPerByte {
    type Target = InternalGasPerByte;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl StorageCostPerByte {
    fn new(x: u64) -> Self {
        StorageCostPerByte(InternalGasPerByte::new(x))
    }
}

/// A list of constant costs of various operations in Sui.
pub struct SuiCostTable {
    /// A flat fee charged for every transaction. This is also the mimmum amount of
    /// gas charged for a transaction.
    min_transaction_cost: FixedCost,
    /// Maximum allowable budget for a transaction.
    pub(crate) max_gas_budget: u64,
    /// Computation cost per byte charged for package publish. This cost is primarily
    /// determined by the cost to verify and link a package. Note that this does not
    /// include the cost of writing the package to the store.
    package_publish_per_byte_cost: ComputationCostPerByte,
    /// Per byte cost to read objects from the store. This is computation cost instead of
    /// storage cost because it does not change the amount of data stored on the db.
    object_read_per_byte_cost: ComputationCostPerByte,
    /// Unit cost of a byte in the storage. This will be used both for charging for
    /// new storage as well as rebating for deleting storage. That is, we expect users to
    /// get full refund on the object storage when it's deleted.
    storage_per_byte_cost: StorageCostPerByte,
    /// Execution cost table to be used.
    pub execution_cost_table: CostTable,
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
            min_transaction_cost: FixedCost::new(c.base_tx_cost_fixed()),
            max_gas_budget: c.max_tx_gas(),
            package_publish_per_byte_cost: ComputationCostPerByte::new(
                c.package_publish_cost_per_byte(),
            ),
            object_read_per_byte_cost: ComputationCostPerByte::new(
                c.obj_access_cost_read_per_byte(),
            ),
            storage_per_byte_cost: StorageCostPerByte::new(c.obj_data_cost_refundable()),
            execution_cost_table: INITIAL_COST_SCHEDULE.clone(),
        }
    }

    pub(crate) fn unmetered() -> Self {
        Self {
            min_transaction_cost: FixedCost::new(0),
            max_gas_budget: u64::MAX,
            package_publish_per_byte_cost: ComputationCostPerByte::new(0),
            object_read_per_byte_cost: ComputationCostPerByte::new(0),
            storage_per_byte_cost: StorageCostPerByte::new(0),
            execution_cost_table: ZERO_COST_SCHEDULE.clone(),
        }
    }

    pub(crate) fn min_gas_budget_external(&self) -> u64 {
        u64::from(to_external(*self.min_transaction_cost))
    }
}

fn to_external(internal_units: InternalGas) -> GasUnits {
    InternalGas::to_unit_round_down(internal_units)
}

#[derive(Debug)]
pub struct SuiGasStatus {
    pub gas_status: GasStatus,
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

    cost_table: SuiCostTable,
}

fn to_internal(external_units: GasUnits) -> InternalGas {
    GasUnits::to_unit(external_units)
}

impl SuiGasStatus {
    fn new(
        move_gas_status: GasStatus,
        gas_budget: u64,
        charge: bool,
        computation_gas_unit_price: GasPrice,
        storage_gas_unit_price: u64,
        cost_table: SuiCostTable,
    ) -> SuiGasStatus {
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
            cost_table,
        }
    }
    pub(crate) fn new_for_testing(
        gas_budget: u64,
        computation_gas_unit_price: u64,
        storage_gas_unit_price: u64,
        cost_table: SuiCostTable,
    ) -> SuiGasStatus {
        let budget_in_unit = gas_budget / computation_gas_unit_price; // truncate the value and move to units
        Self::new(
            GasStatus::new(cost_table.execution_cost_table.clone(), GasUnits::new(budget_in_unit)),
            budget_in_unit,
            true,
            computation_gas_unit_price.into(),
            storage_gas_unit_price,
            cost_table,
        )
    }

    pub(crate) fn new_with_budget(
        gas_budget: u64,
        computation_gas_unit_price: u64,
        config: &ProtocolConfig,
    ) -> SuiGasStatus {
        let storage_gas_unit_price: GasPrice = config.storage_gas_price().into();
         // truncate the value and move to units
        let budget_in_unit = gas_budget / computation_gas_unit_price;
        let sui_cost_table = SuiCostTable::new(config);
        Self::new(
            GasStatus::new(sui_cost_table.execution_cost_table.clone(), GasUnits::new(budget_in_unit)),
            budget_in_unit,
            true,
            computation_gas_unit_price.into(),
            storage_gas_unit_price.into(),
            sui_cost_table,
        )
    }

    pub(crate) fn new_unmetered() -> SuiGasStatus {
        Self::new(
            GasStatus::new_unmetered(),
            0,
            false,
            0.into(),
            0,
            SuiCostTable::unmetered(),
        )
    }

    fn charge_storage_mutation_with_rebate(
        &mut self,
        new_size: usize,
        storage_rebate: SuiGas,
    ) -> Result<u64, ExecutionError> {
        if self.is_unmetered() {
            return Ok(0);
        }

        let storage_cost =
            NumBytes::new(new_size as u64).mul(*self.cost_table.storage_per_byte_cost);
        self.deduct_storage_cost(&storage_cost).map(|gu| {
            self.storage_rebate.add_assign(storage_rebate);
            gu.into()
        })
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

    fn gas_used_in_gas_units(&self) -> GasUnits {
        let remaining_gas = self.gas_status.remaining_gas();
        self.init_budget
            .checked_sub(remaining_gas)
            .expect("Subtraction overflowed")
    }
}

impl SuiGasStatusAPI for SuiGasStatus {
    fn is_unmetered(&self) -> bool {
        !self.charge
    }

    fn move_gas_status(&mut self) -> &mut GasStatus {
        &mut self.gas_status
    }

    fn bucketize_computation(&mut self) -> Result<(), ExecutionError> {
        let computation_cost: u64 = self.gas_used();
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

    /// Returns the final (computation cost, storage cost, storage rebate) of the gas meter.
    /// We use initial budget, combined with remaining gas and storage cost to derive
    /// computation cost.
    fn summary(&self) -> GasCostSummary {
        let remaining_gas = self.gas_status.remaining_gas();
        let storage_cost = self.storage_gas_units;
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
            // gas model v1 does not use non refundable fees
            non_refundable_storage_fee: 0,
        }
    }

    fn gas_budget(&self) -> u64 {
        // MUSTFIX: Properly compute gas budget
        let max_gas_unit_price =
            std::cmp::max(self.computation_gas_unit_price, self.storage_gas_unit_price);
        self.init_budget.mul(max_gas_unit_price).into()
    }

    fn storage_rebate(&self) -> u64 {
        self.storage_rebate.into()
    }

    fn storage_gas_units(&self) -> u64 {
        self.storage_gas_units.into()
    }

    fn unmetered_storage_rebate(&self) -> u64 {
        unreachable!("unmetered_storage_rebate should not be called in v1 gas model");
    }

    fn gas_used(&self) -> u64 {
        self.gas_used_in_gas_units().into()
    }

    fn reset_storage_cost_and_rebate(&mut self) {
        self.storage_gas_units = GasQuantity::zero();
        self.storage_rebate = GasQuantity::zero();
    }

    fn charge_storage_read(&mut self, size: usize) -> Result<(), ExecutionError> {
        let cost = NumBytes::new(size as u64).mul(*self.cost_table.object_read_per_byte_cost);
        self.deduct_computation_cost(&cost)
    }

    fn charge_storage_mutation(
        &mut self,
        new_size: usize,
        storage_rebate: u64,
    ) -> Result<u64, ExecutionError> {
        self.charge_storage_mutation_with_rebate(new_size, storage_rebate.into())
    }

    fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError> {
        let computation_cost =
            NumBytes::new(size as u64).mul(*self.cost_table.package_publish_per_byte_cost);

        self.deduct_computation_cost(&computation_cost)
    }

    fn charge_storage_and_rebate(&mut self) -> Result<(), ExecutionError> {
        unreachable!("charge_storage_and_rebate should not be called in v1 gas model");
    }

    fn track_storage_mutation(&mut self, _new_size: usize, _storage_rebate: u64) -> u64 {
        unreachable!("track_storage_mutation should not be called in v1 gas model");
    }

    fn adjust_computation_on_out_of_gas(&mut self) {
        unreachable!("adjust_computation_on_out_of_gas should not be called in v1 gas model");
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

}
