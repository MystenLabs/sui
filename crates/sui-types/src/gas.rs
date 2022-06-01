// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{SuiError, SuiResult},
    gas_coin::GasCoin,
    object::Object,
};
use move_core_types::gas_schedule::{
    AbstractMemorySize, GasAlgebra, GasCarrier, GasPrice, GasUnits, InternalGasUnits,
};
use move_vm_types::gas_schedule::{GasStatus, INITIAL_COST_SCHEDULE};
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

macro_rules! ok_or_gas_error {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            Err(SuiError::InsufficientGas { error: $e })
        } else {
            Ok(())
        }
    };
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GasCostSummary {
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
}

impl GasCostSummary {
    pub fn gas_used(&self) -> u64 {
        self.computation_cost + self.storage_cost
    }
}

/// ComputationCost is a newtype wrapper of InternalGasUnits
/// to ensure a value of this type is used specifically for computation cost.
/// Anything that does not change the amount of bytes stored in the authority data store
/// will charge ComputationCost.
struct ComputationCost(InternalGasUnits<GasCarrier>);

impl ComputationCost {
    /// Some computations are also linear to the size of data it operates on.
    pub fn with_size(&self, size: usize) -> Self {
        Self(self.0.mul(AbstractMemorySize::new(size as u64)))
    }
}

/// StorageCost is a newtype wrapper of InternalGasUnits
/// to ensure a value of this type is used specifically for storage cost.
/// Anything that changes the amount of bytes stored in the authority data store
/// will charge StorageCost.
struct StorageCost(InternalGasUnits<GasCarrier>);

impl StorageCost {
    pub fn with_size(&self, size: usize) -> Self {
        Self(self.0.mul(AbstractMemorySize::new(size as u64)))
    }
}

/// A list of constant costs of various operations in Sui.
struct SuiCostTable {
    /// A flat fee charged for every transaction. This is also the mimmum amount of
    /// gas charged for a transaction.
    pub min_transaction_cost: ComputationCost,
    /// Computation cost per byte charged for package publish. This cost is primarily
    /// determined by the cost to verify and link a package. Note that this does not
    /// include the cost of writing the package to the store.
    pub package_publish_per_byte_cost: ComputationCost,
    /// Per byte cost to read objects from the store. This is computation cost instead of
    /// storage cost because it does not change the amount of data stored on the db.
    pub object_read_per_byte_cost: ComputationCost,
    /// Per byte cost to write objects to the store. This is computation cost instead of
    /// storage cost because it does not change the amount of data stored on the db.
    pub object_mutation_per_byte_cost: ComputationCost,
    /// Cost to use shared objects in a transaction, which requires full consensus.
    pub consensus_cost: ComputationCost,

    /// Unit cost of a byte in the storage. This will be used both for charging for
    /// new storage as well as rebating for deleting storage. That is, we expect users to
    /// get full refund on the object storage when it's deleted.
    /// TODO: We should introduce a flat fee on storage that does not get refunded even
    /// when objects are deleted. This cost covers the cost of storing transaction metadata
    /// which will always be there even after the objects are deleted.
    pub storage_per_byte_cost: StorageCost,
}

// TODO: The following numbers are arbitrary at this point.
static INIT_SUI_COST_TABLE: Lazy<SuiCostTable> = Lazy::new(|| SuiCostTable {
    min_transaction_cost: ComputationCost(InternalGasUnits::new(10000)),
    package_publish_per_byte_cost: ComputationCost(InternalGasUnits::new(80)),
    object_read_per_byte_cost: ComputationCost(InternalGasUnits::new(15)),
    object_mutation_per_byte_cost: ComputationCost(InternalGasUnits::new(40)),
    consensus_cost: ComputationCost(InternalGasUnits::new(100000)),

    storage_per_byte_cost: StorageCost(InternalGasUnits::new(100)),
});

pub static MAX_GAS_BUDGET: Lazy<u64> =
    Lazy::new(|| to_external(InternalGasUnits::new(u64::MAX)).get());

pub static MIN_GAS_BUDGET: Lazy<u64> =
    Lazy::new(|| to_external(INIT_SUI_COST_TABLE.min_transaction_cost.0).get());

fn to_external(internal_units: InternalGasUnits<GasCarrier>) -> GasUnits<GasCarrier> {
    let consts = &INITIAL_COST_SCHEDULE.gas_constants;
    consts.to_external_units(internal_units)
}

fn to_internal(external_units: GasUnits<GasCarrier>) -> InternalGasUnits<GasCarrier> {
    let consts = &INITIAL_COST_SCHEDULE.gas_constants;
    consts.to_internal_units(external_units)
}

pub struct SuiGasStatus<'a> {
    gas_status: GasStatus<'a>,
    init_budget: GasUnits<GasCarrier>,
    charge: bool,
    computation_gas_unit_price: GasPrice<GasCarrier>,
    storage_gas_unit_price: GasPrice<GasCarrier>,
    /// storage_cost is the total storage gas units charged so far, due to writes into storage.
    /// It will be multiplied by the storage gas unit price in the end to obtain the Sui cost.
    storage_cost: GasUnits<GasCarrier>,
    /// storage_rebate is the total storage rebate (in Sui) accumulated in this transaction.
    /// It's directly coming from each mutated object's storage rebate field, which
    /// was the storage cost paid when the object was last mutated. It is not affected
    /// by the current storage gas unit price.
    storage_rebate: GasCarrier,
}

impl<'a> SuiGasStatus<'a> {
    pub fn new_with_budget(
        gas_budget: u64,
        computation_gas_unit_price: GasCarrier,
        storage_gas_unit_price: GasCarrier,
    ) -> SuiGasStatus<'a> {
        Self::new(
            GasStatus::new(&INITIAL_COST_SCHEDULE, GasUnits::new(gas_budget)),
            gas_budget,
            true,
            computation_gas_unit_price,
            storage_gas_unit_price,
        )
    }

    pub fn new_unmetered() -> SuiGasStatus<'a> {
        Self::new(GasStatus::new_unmetered(), 0, false, 0, 0)
    }

    pub fn is_unmetered(&self) -> bool {
        !self.charge
    }

    pub fn get_move_gas_status(&mut self) -> &mut GasStatus<'a> {
        &mut self.gas_status
    }

    pub fn charge_min_tx_gas(&mut self) -> SuiResult {
        self.deduct_computation_cost(&INIT_SUI_COST_TABLE.min_transaction_cost)
    }

    pub fn charge_consensus(&mut self) -> SuiResult {
        self.deduct_computation_cost(&INIT_SUI_COST_TABLE.consensus_cost)
    }

    pub fn charge_publish_package(&mut self, size: usize) -> SuiResult {
        let computation_cost = INIT_SUI_COST_TABLE
            .package_publish_per_byte_cost
            .with_size(size);
        self.deduct_computation_cost(&computation_cost)
    }

    pub fn charge_storage_read(&mut self, size: usize) -> SuiResult {
        let cost = INIT_SUI_COST_TABLE
            .object_read_per_byte_cost
            .with_size(size);
        self.deduct_computation_cost(&cost)
    }

    pub fn charge_storage_mutation(
        &mut self,
        old_size: usize,
        new_size: usize,
        storage_rebate: GasCarrier,
    ) -> SuiResult<u64> {
        if self.is_unmetered() {
            return Ok(0);
        }

        // Computation cost of a mutation is charged based on the sum of the old and new size.
        // This is because to update an object in the store, we have to erase the old one and
        // write a new one.
        let cost = INIT_SUI_COST_TABLE
            .object_mutation_per_byte_cost
            .with_size(old_size + new_size);
        self.deduct_computation_cost(&cost)?;

        self.storage_rebate += storage_rebate;

        let storage_cost = INIT_SUI_COST_TABLE
            .storage_per_byte_cost
            .with_size(new_size);
        self.deduct_storage_cost(&storage_cost)
    }

    /// This function is only called during testing, where we need to mock
    /// Move VM charging gas.
    pub fn charge_vm_exec_test_only(&mut self, cost: u64) -> SuiResult {
        self.gas_status.deduct_gas(InternalGasUnits::new(cost))?;
        Ok(())
    }

    /// Returns the final (computation cost, storage cost, storage rebate) of the gas meter.
    /// We use initial budget, combined with remaining gas and storage cost to derive
    /// computation cost.
    pub fn summary(&self, succeeded: bool) -> GasCostSummary {
        let remaining_gas = self.gas_status.remaining_gas();
        let storage_cost = self.storage_cost;
        let computation_cost = self.init_budget.sub(remaining_gas).sub(storage_cost);
        let computation_cost_in_sui = computation_cost.mul(self.computation_gas_unit_price).get();
        if succeeded {
            GasCostSummary {
                computation_cost: computation_cost_in_sui,
                storage_cost: storage_cost.mul(self.storage_gas_unit_price).get(),
                storage_rebate: self.storage_rebate,
            }
        } else {
            // If execution failed, no storage creation/deletion will materialize in the store.
            // Hence they should be 0.
            GasCostSummary {
                computation_cost: computation_cost_in_sui,
                storage_cost: 0,
                storage_rebate: 0,
            }
        }
    }

    fn new(
        move_gas_status: GasStatus<'a>,
        gas_budget: u64,
        charge: bool,
        computation_gas_unit_price: GasCarrier,
        storage_gas_unit_price: u64,
    ) -> SuiGasStatus<'a> {
        SuiGasStatus {
            gas_status: move_gas_status,
            init_budget: GasUnits::new(gas_budget),
            charge,
            computation_gas_unit_price: GasPrice::new(computation_gas_unit_price),
            storage_gas_unit_price: GasPrice::new(storage_gas_unit_price),
            storage_cost: GasUnits::new(0),
            storage_rebate: 0,
        }
    }

    fn deduct_computation_cost(&mut self, cost: &ComputationCost) -> SuiResult {
        if self.gas_status.deduct_gas(cost.0).is_err() {
            Err(SuiError::InsufficientGas {
                error: "Ran out of gas while deducting computation cost".to_owned(),
            })
        } else {
            Ok(())
        }
    }

    fn deduct_storage_cost(&mut self, cost: &StorageCost) -> SuiResult<GasCarrier> {
        if self.is_unmetered() {
            return Ok(0);
        }
        let ext_cost = to_external(cost.0);
        let charge_amount = to_internal(ext_cost);
        let remaining_gas = self.gas_status.remaining_gas();
        if self.gas_status.deduct_gas(charge_amount).is_err() {
            debug_assert_eq!(self.gas_status.remaining_gas().get(), 0);
            // Even when we run out of gas, we still keep track of the storage_cost change,
            // so that at the end, we could still use it to accurately derive the
            // computation cost.
            self.storage_cost = self.storage_cost.add(remaining_gas);
            Err(SuiError::InsufficientGas {
                error: "Ran out of gas while deducting storage cost".to_owned(),
            })
        } else {
            self.storage_cost = self.storage_cost.add(ext_cost);
            Ok(ext_cost.mul(self.storage_gas_unit_price).get())
        }
    }
}

/// Check whether the given gas_object and gas_budget is legit:
/// 1. If the gas object is owned.
/// 2. If it's enough to pay the flat minimum transaction fee
/// 3. If it's less than the max gas budget allowed
/// 4. If the gas_object actually has enough balance to pay for the budget.
pub fn check_gas_balance(gas_object: &Object, gas_budget: u64) -> SuiResult {
    ok_or_gas_error!(
        gas_object.is_owned(),
        "Gas object must be owned Move object".to_owned()
    )?;
    ok_or_gas_error!(
        gas_budget <= *MAX_GAS_BUDGET,
        format!("Gas budget set too high; maximum is {}", *MAX_GAS_BUDGET)
    )?;
    ok_or_gas_error!(
        gas_budget >= *MIN_GAS_BUDGET,
        format!(
            "Gas budget is {}, smaller than minimum requirement {}",
            gas_budget, *MIN_GAS_BUDGET
        )
    )?;

    let balance = get_gas_balance(gas_object)?;
    ok_or_gas_error!(
        balance >= gas_budget,
        format!("Gas balance is {balance}, not enough to pay {gas_budget}")
    )
}

/// Create a new gas status with the given `gas_budget`, and charge the transaction flat fee.
pub fn start_gas_metering(
    gas_budget: u64,
    computation_gas_unit_price: u64,
    storage_gas_unit_price: u64,
) -> SuiResult<SuiGasStatus<'static>> {
    let mut gas_status = SuiGasStatus::new_with_budget(
        gas_budget,
        computation_gas_unit_price,
        storage_gas_unit_price,
    );
    // Charge the flat transaction fee.
    gas_status.charge_min_tx_gas()?;
    Ok(gas_status)
}

/// Subtract the gas balance of \p gas_object by \p amount.
/// This function should never fail, since we checked that the budget is always
/// less than balance, and the amount is capped at the budget.
pub fn deduct_gas(gas_object: &mut Object, deduct_amount: u64, rebate_amount: u64) {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    debug_assert!(balance >= deduct_amount);
    let new_gas_coin = GasCoin::new(
        *gas_coin.id(),
        gas_object.version(),
        balance + rebate_amount - deduct_amount,
    );
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    move_object.update_contents(bcs::to_bytes(&new_gas_coin).unwrap());
}

pub fn refund_gas(gas_object: &mut Object, amount: u64) {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    let new_gas_coin = GasCoin::new(*gas_coin.id(), gas_object.version(), balance + amount);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    move_object.update_contents(bcs::to_bytes(&new_gas_coin).unwrap());
}

pub fn get_gas_balance(gas_object: &Object) -> SuiResult<u64> {
    Ok(GasCoin::try_from(gas_object)?.value())
}
