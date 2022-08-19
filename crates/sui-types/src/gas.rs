// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{ExecutionError, ExecutionErrorKind},
    error::{SuiError, SuiResult},
    gas_coin::GasCoin,
    object::{Object, Owner},
};
use move_core_types::{
    gas_algebra::{GasQuantity, InternalGas, InternalGasPerByte, NumBytes, UnitDiv},
    vm_status::StatusCode,
};
use move_vm_test_utils::gas_schedule::{GasStatus, GasUnit, INITIAL_COST_SCHEDULE};
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    ops::{Add, Mul},
};

pub type GasUnits = GasQuantity<GasUnit>;
pub enum GasPriceUnit {}
pub enum SuiGasUnit {}

pub type ComputeGasPricePerUnit = GasQuantity<UnitDiv<GasUnit, GasUnit>>;

pub type GasPrice = GasQuantity<GasPriceUnit>;
pub type SuiGas = GasQuantity<SuiGasUnit>;

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

    /// Get net gas usage, positive number means used gas; negative number means refund.
    pub fn net_gas_usage(&self) -> i64 {
        self.gas_used() as i64 - self.storage_rebate as i64
    }
}

/// ComputationCost is a newtype wrapper of InternalGas
/// to ensure a value of this type is used specifically for computation cost.
/// Anything that does not change the amount of bytes stored in the authority data store
/// will charge ComputationCost.
struct ComputationCost(InternalGasPerByte);

impl ComputationCost {
    /// Some computations are also linear to the size of data it operates on.
    pub fn with_size(&self, size: usize) -> Self {
        // TODO: this ia a hacky way to keep things compat. Normally the units here dont match
        Self(InternalGasPerByte::new(u64::from(
            NumBytes::new(size as u64).mul(self.0),
        )))
    }
}

/// StorageCost is a newtype wrapper of InternalGas
/// to ensure a value of this type is used specifically for storage cost.
/// Anything that changes the amount of bytes stored in the authority data store
/// will charge StorageCost.
struct StorageCost(InternalGasPerByte);

impl StorageCost {
    pub fn with_size(&self, size: usize) -> Self {
        // TODO: this ia a hacky way to keep things compat. Normally the units here dont match
        Self(InternalGasPerByte::new(u64::from(
            NumBytes::new(size as u64).mul(self.0),
        )))
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
    min_transaction_cost: ComputationCost(InternalGasPerByte::new(10000)),
    package_publish_per_byte_cost: ComputationCost(InternalGasPerByte::new(80)),
    object_read_per_byte_cost: ComputationCost(InternalGasPerByte::new(15)),
    object_mutation_per_byte_cost: ComputationCost(InternalGasPerByte::new(40)),
    consensus_cost: ComputationCost(InternalGasPerByte::new(100000)),

    storage_per_byte_cost: StorageCost(InternalGasPerByte::new(100)),
});

pub static MAX_GAS_BUDGET: Lazy<u64> = Lazy::new(|| to_external(InternalGas::new(u64::MAX)).into());

pub static MIN_GAS_BUDGET: Lazy<u64> = Lazy::new(|| {
    to_external(NumBytes::new(1).mul(INIT_SUI_COST_TABLE.min_transaction_cost.0)).into()
});

fn to_external(internal_units: InternalGas) -> GasUnits {
    InternalGas::to_unit_round_down(internal_units)
}

fn to_internal(external_units: GasUnits) -> InternalGas {
    GasUnits::to_unit(external_units)
}

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
}

impl<'a> SuiGasStatus<'a> {
    pub fn new_with_budget(
        gas_budget: u64,
        computation_gas_unit_price: GasPrice,
        storage_gas_unit_price: GasPrice,
    ) -> SuiGasStatus<'a> {
        Self::new(
            GasStatus::new(&INITIAL_COST_SCHEDULE, GasUnits::new(gas_budget)),
            gas_budget,
            true,
            computation_gas_unit_price,
            storage_gas_unit_price.into(),
        )
    }

    pub fn new_unmetered() -> SuiGasStatus<'a> {
        Self::new(GasStatus::new_unmetered(), 0, false, 0.into(), 0)
    }

    pub fn is_unmetered(&self) -> bool {
        !self.charge
    }

    pub fn get_move_gas_status(&mut self) -> &mut GasStatus<'a> {
        &mut self.gas_status
    }

    pub fn charge_min_tx_gas(&mut self) -> Result<(), ExecutionError> {
        self.deduct_computation_cost(&INIT_SUI_COST_TABLE.min_transaction_cost)
    }

    pub fn charge_consensus(&mut self) -> Result<(), ExecutionError> {
        self.deduct_computation_cost(&INIT_SUI_COST_TABLE.consensus_cost)
    }

    pub fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError> {
        let computation_cost = INIT_SUI_COST_TABLE
            .package_publish_per_byte_cost
            .with_size(size);
        self.deduct_computation_cost(&computation_cost)
    }

    pub fn charge_storage_read(&mut self, size: usize) -> Result<(), ExecutionError> {
        let cost = INIT_SUI_COST_TABLE
            .object_read_per_byte_cost
            .with_size(size);
        self.deduct_computation_cost(&cost)
    }

    pub fn charge_storage_mutation(
        &mut self,
        old_size: usize,
        new_size: usize,
        storage_rebate: SuiGas,
    ) -> Result<u64, ExecutionError> {
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
        self.deduct_storage_cost(&storage_cost).map(|q| q.into())
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
    pub fn summary(&self, succeeded: bool) -> GasCostSummary {
        let remaining_gas = self.gas_status.remaining_gas();
        let storage_cost = self.storage_gas_units;
        // TODO: handle overflow how?
        let computation_cost = self
            .init_budget
            .checked_sub(remaining_gas)
            .expect("Subtraction overflowed")
            .checked_sub(storage_cost)
            .expect("Subtraction overflowed");
        let computation_cost_in_sui = computation_cost.mul(self.computation_gas_unit_price).into();
        if succeeded {
            GasCostSummary {
                computation_cost: computation_cost_in_sui,
                storage_cost: storage_cost.mul(self.storage_gas_unit_price).into(),
                storage_rebate: self.storage_rebate.into(),
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
        computation_gas_unit_price: GasPrice,
        storage_gas_unit_price: u64,
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
        }
    }

    fn deduct_computation_cost(&mut self, cost: &ComputationCost) -> Result<(), ExecutionError> {
        self.gas_status
            .deduct_gas(NumBytes::new(1).mul(cost.0))
            .map_err(|e| {
                debug_assert_eq!(e.major_status(), StatusCode::OUT_OF_GAS);
                ExecutionErrorKind::InsufficientGas.into()
            })
    }

    fn deduct_storage_cost(&mut self, cost: &StorageCost) -> Result<GasUnits, ExecutionError> {
        if self.is_unmetered() {
            return Ok(0.into());
        }
        let ext_cost =
            to_external(NumBytes::new(1).mul(InternalGasPerByte::new(u64::from(cost.0))));
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
}

/// Check whether the given gas_object and gas_budget is legit:
/// 1. If the gas object has an address owner.
/// 2. If it's enough to pay the flat minimum transaction fee
/// 3. If it's less than the max gas budget allowed
/// 4. If the gas_object actually has enough balance to pay for the budget.
pub fn check_gas_balance(
    gas_object: &Object,
    gas_budget: u64,
    gas_price: u64,
    extra_amount: u64,
) -> SuiResult {
    ok_or_gas_error!(
        matches!(gas_object.owner, Owner::AddressOwner(_)),
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
    let total_amount = (gas_budget as u128) * (gas_price as u128) + extra_amount as u128;
    ok_or_gas_error!(
        (balance as u128) >= total_amount,
        format!("Gas balance is {balance}, not enough to pay {total_amount} with gas price of {gas_price}")
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
        computation_gas_unit_price.into(),
        storage_gas_unit_price.into(),
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
    let new_gas_coin = GasCoin::new(*gas_coin.id(), balance + rebate_amount - deduct_amount);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    move_object.update_contents_and_increment_version(bcs::to_bytes(&new_gas_coin).unwrap());
}

pub fn refund_gas(gas_object: &mut Object, amount: u64) {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    let new_gas_coin = GasCoin::new(*gas_coin.id(), balance + amount);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    move_object.update_contents_and_increment_version(bcs::to_bytes(&new_gas_coin).unwrap());
}

pub fn get_gas_balance(gas_object: &Object) -> SuiResult<u64> {
    Ok(GasCoin::try_from(gas_object)?.value())
}
