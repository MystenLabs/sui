// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{SuiError, SuiResult},
    gas_coin::GasCoin,
    object::Object,
};
use move_core_types::gas_schedule::{
    AbstractMemorySize, GasAlgebra, GasCarrier, GasUnits, InternalGasUnits,
};
use move_vm_types::gas_schedule::{GasStatus, INITIAL_COST_SCHEDULE};
use once_cell::sync::Lazy;
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

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
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
    Lazy::new(|| u64::MAX / INITIAL_COST_SCHEDULE.gas_constants.gas_unit_scaling_factor);

pub static MIN_GAS_BUDGET: Lazy<u64> = Lazy::new(|| {
    let consts = &INITIAL_COST_SCHEDULE.gas_constants;
    consts
        .to_external_units(INIT_SUI_COST_TABLE.min_transaction_cost.0)
        .get()
});

pub struct SuiGasStatus<'a> {
    gas_status: GasStatus<'a>,
    init_budget: u64,
    storage_cost: InternalGasUnits<GasCarrier>,
    storage_rebate: InternalGasUnits<GasCarrier>,
}

impl<'a> SuiGasStatus<'a> {
    pub fn new_with_budget(gas_budget: u64) -> SuiGasStatus<'a> {
        Self::new(
            GasStatus::new(&INITIAL_COST_SCHEDULE, GasUnits::new(gas_budget)),
            gas_budget,
        )
    }

    pub fn new_unmetered() -> SuiGasStatus<'a> {
        Self::new(GasStatus::new_unmetered(), 0)
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

    pub fn charge_storage_mutation(&mut self, old_size: usize, new_size: usize) -> SuiResult {
        // Computation cost of a mutation is charged based on the sum of the old and new size.
        // This is because to update an object in the store, we have to erase the old one and
        // write a new one.
        let cost = INIT_SUI_COST_TABLE
            .object_mutation_per_byte_cost
            .with_size(old_size + new_size);
        self.deduct_computation_cost(&cost)?;

        /// TODO: For rebates, what we want is to keep track of most recent storage
        /// cost (in Sui) for each object. whenever we mutate an object, we always rebate the old
        /// cost, and recharge based on the current storage gas price.
        use std::cmp::Ordering;
        match new_size.cmp(&old_size) {
            Ordering::Greater => {
                let cost = INIT_SUI_COST_TABLE
                    .storage_per_byte_cost
                    .with_size(new_size - old_size);
                self.deduct_storage_cost(&cost)
            }
            Ordering::Less => {
                self.rebate_storage_deletion(old_size - new_size);
                Ok(())
            }
            Ordering::Equal => {
                // Do nothing about storage cost if old_size == new_size.
                Ok(())
            }
        }
    }

    /// Returns the final (computation cost, storage cost, storage rebate) of the gas meter.
    /// We use initial budget, combined with remaining gas and storage cost to derive
    /// computation cost.
    pub fn summary(&self, succeeded: bool) -> GasCostSummary {
        let consts = &INITIAL_COST_SCHEDULE.gas_constants;
        let remaining_gas = self.gas_status.remaining_gas().get();
        let storage_cost = consts.to_external_units(self.storage_cost).get();
        let computation_cost = self.init_budget - remaining_gas - storage_cost;
        let storage_rebate = consts.to_external_units(self.storage_rebate).get();
        if succeeded {
            GasCostSummary {
                computation_cost,
                storage_cost,
                storage_rebate,
            }
        } else {
            // If execution failed, no storage creation/deletion will materialize in the store.
            // Hence they should be 0.
            GasCostSummary {
                computation_cost,
                storage_cost: 0,
                storage_rebate: 0,
            }
        }
    }

    fn new(move_gas_status: GasStatus<'a>, gas_budget: u64) -> SuiGasStatus<'a> {
        SuiGasStatus {
            gas_status: move_gas_status,
            init_budget: gas_budget,
            storage_cost: InternalGasUnits::new(0),
            storage_rebate: InternalGasUnits::new(0),
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

    fn deduct_storage_cost(&mut self, cost: &StorageCost) -> SuiResult {
        let remaining_gas = self.gas_status.remaining_gas();
        if self.gas_status.deduct_gas(cost.0).is_err() {
            debug_assert_eq!(self.gas_status.remaining_gas().get(), 0);
            self.storage_cost = self.storage_cost.add(
                INITIAL_COST_SCHEDULE
                    .gas_constants
                    .to_internal_units(remaining_gas),
            );
            Err(SuiError::InsufficientGas {
                error: "Ran out of gas while deducting storage cost".to_owned(),
            })
        } else {
            self.storage_cost = self.storage_cost.add(cost.0);
            Ok(())
        }
    }

    fn rebate_storage_deletion(&mut self, size: usize) {
        let rebate = INIT_SUI_COST_TABLE.storage_per_byte_cost.with_size(size);
        self.storage_rebate = self.storage_rebate.add(rebate.0);
    }
}

/// Check whether the given gas_object and gas_budget is legit:
/// 1. If the gas object is owned.
/// 2. If it's enough to pay the flat minimum transaction fee
/// 3. If it's less than the max gas budget allowed
/// 4. If the gas_object actually has enough balance to pay for the budget.
pub fn check_gas_balance(gas_object: &Object, gas_budget: u64) -> SuiResult {
    ok_or_gas_error!(
        !gas_object.is_shared(),
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
pub fn start_gas_metering(gas_budget: u64) -> SuiResult<SuiGasStatus<'static>> {
    let mut gas_status = SuiGasStatus::new_with_budget(gas_budget);
    // Charge the flat transaction fee.
    gas_status.charge_min_tx_gas()?;
    Ok(gas_status)
}

/// Subtract the gas balance of \p gas_object by \p amount.
/// This function should never fail, since we checked that the budget is always
/// less than balance, and the amount is capped at the budget.
pub fn deduct_gas(gas_object: &mut Object, amount: u64) {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    debug_assert!(balance >= amount);
    let new_gas_coin = GasCoin::new(*gas_coin.id(), gas_object.version(), balance - amount);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    move_object.update_contents(bcs::to_bytes(&new_gas_coin).unwrap());
}

pub fn get_gas_balance(gas_object: &Object) -> SuiResult<u64> {
    Ok(GasCoin::try_from(gas_object)?.value())
}
