// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_binary_format::errors::{PartialVMError, PartialVMResult};

use move_core_types::gas_algebra::{AbstractMemorySize, InternalGas};

use move_core_types::vm_status::StatusCode;
use once_cell::sync::Lazy;

use crate::gas_model::units_types::{CostTable, Gas, GasCost};

use super::gas_predicates::charge_input_as_memory;

/// VM flat fee
pub const VM_FLAT_FEE: Gas = Gas::new(8_000);

/// The size in bytes for a non-string or address constant on the stack
pub const CONST_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);

/// The size in bytes for a reference on the stack
pub const REFERENCE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);

/// The size of a struct in bytes
pub const STRUCT_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);

/// The size of a vector (without its containing data) in bytes
pub const VEC_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);

/// For exists checks on data that doesn't exists this is the multiplier that is used.
pub const MIN_EXISTS_DATA_SIZE: AbstractMemorySize = AbstractMemorySize::new(100);

pub static ZERO_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(zero_cost_schedule);

pub static INITIAL_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(initial_cost_schedule_v1);

/// The Move VM implementation of state for gas metering.
///
/// Initialize with a `CostTable` and the gas provided to the transaction.
/// Provide all the proper guarantees about gas metering in the Move VM.
///
/// Every client must use an instance of this type to interact with the Move VM.
#[allow(dead_code)]
#[derive(Debug)]
pub struct GasStatus {
    pub gas_model_version: u64,
    cost_table: CostTable,
    pub gas_left: InternalGas,
    gas_price: u64,
    initial_budget: InternalGas,
    pub charge: bool,

    // The current height of the operand stack, and the maximal height that it has reached.
    stack_height_high_water_mark: u64,
    stack_height_current: u64,
    stack_height_next_tier_start: Option<u64>,
    stack_height_current_tier_mult: u64,

    // The current (abstract) size  of the operand stack and the maximal size that it has reached.
    stack_size_high_water_mark: u64,
    stack_size_current: u64,
    stack_size_next_tier_start: Option<u64>,
    stack_size_current_tier_mult: u64,

    // The total number of bytecode instructions that have been executed in the transaction.
    instructions_executed: u64,
    instructions_next_tier_start: Option<u64>,
    instructions_current_tier_mult: u64,

    pub num_native_calls: u64,
}

impl GasStatus {
    /// Initialize the gas state with metering enabled.
    ///
    /// Charge for every operation and fail when there is no more gas to pay for operations.
    /// This is the instantiation that must be used when executing a user script.
    pub fn new(cost_table: CostTable, budget: u64, gas_price: u64, gas_model_version: u64) -> Self {
        assert!(gas_price > 0, "gas price cannot be 0");
        let budget_in_unit = budget / gas_price;
        let gas_left = Self::to_internal_units(budget_in_unit);
        let (stack_height_current_tier_mult, stack_height_next_tier_start) =
            cost_table.stack_height_tier(0);
        let (stack_size_current_tier_mult, stack_size_next_tier_start) =
            cost_table.stack_size_tier(0);
        let (instructions_current_tier_mult, instructions_next_tier_start) =
            cost_table.instruction_tier(0);
        Self {
            gas_model_version,
            gas_left,
            gas_price,
            initial_budget: gas_left,
            cost_table,
            charge: true,
            stack_height_high_water_mark: 0,
            stack_height_current: 0,
            stack_size_high_water_mark: 0,
            stack_size_current: 0,
            instructions_executed: 0,
            stack_height_current_tier_mult,
            stack_size_current_tier_mult,
            instructions_current_tier_mult,
            stack_height_next_tier_start,
            stack_size_next_tier_start,
            instructions_next_tier_start,
            num_native_calls: 0,
        }
    }

    /// Initialize the gas state with metering disabled.
    ///
    /// It should be used by clients in very specific cases and when executing system
    /// code that does not have to charge the user.
    pub fn new_unmetered() -> Self {
        Self {
            gas_model_version: 4,
            gas_left: InternalGas::new(0),
            gas_price: 1,
            initial_budget: InternalGas::new(0),
            cost_table: ZERO_COST_SCHEDULE.clone(),
            charge: false,
            stack_height_high_water_mark: 0,
            stack_height_current: 0,
            stack_size_high_water_mark: 0,
            stack_size_current: 0,
            instructions_executed: 0,
            stack_height_current_tier_mult: 0,
            stack_size_current_tier_mult: 0,
            instructions_current_tier_mult: 0,
            stack_height_next_tier_start: None,
            stack_size_next_tier_start: None,
            instructions_next_tier_start: None,
            num_native_calls: 0,
        }
    }

    const INTERNAL_UNIT_MULTIPLIER: u64 = 1000;

    fn to_internal_units(val: u64) -> InternalGas {
        InternalGas::new(val * Self::INTERNAL_UNIT_MULTIPLIER)
    }

    #[allow(dead_code)]
    fn to_mist(&self, val: InternalGas) -> u64 {
        let gas: Gas = InternalGas::to_unit_round_down(val);
        u64::from(gas) * self.gas_price
    }

    pub fn push_stack(&mut self, pushes: u64) -> PartialVMResult<()> {
        match self.stack_height_current.checked_add(pushes) {
            // We should never hit this.
            None => return Err(PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW)),
            Some(new_height) => {
                if new_height > self.stack_height_high_water_mark {
                    self.stack_height_high_water_mark = new_height;
                }
                self.stack_height_current = new_height;
            }
        }

        if let Some(stack_height_tier_next) = self.stack_height_next_tier_start
            && self.stack_height_current > stack_height_tier_next
        {
            let (next_mul, next_tier) =
                self.cost_table.stack_height_tier(self.stack_height_current);
            self.stack_height_current_tier_mult = next_mul;
            self.stack_height_next_tier_start = next_tier;
        }

        Ok(())
    }

    pub fn pop_stack(&mut self, pops: u64) {
        self.stack_height_current = self.stack_height_current.saturating_sub(pops);
    }

    pub fn increase_instruction_count(&mut self, amount: u64) -> PartialVMResult<()> {
        match self.instructions_executed.checked_add(amount) {
            None => return Err(PartialVMError::new(StatusCode::PC_OVERFLOW)),
            Some(new_pc) => {
                self.instructions_executed = new_pc;
            }
        }

        if let Some(instr_tier_next) = self.instructions_next_tier_start
            && self.instructions_executed > instr_tier_next
        {
            let (instr_cost, next_tier) =
                self.cost_table.instruction_tier(self.instructions_executed);
            self.instructions_current_tier_mult = instr_cost;
            self.instructions_next_tier_start = next_tier;
        }

        Ok(())
    }

    pub fn increase_stack_size(&mut self, size_amount: u64) -> PartialVMResult<()> {
        match self.stack_size_current.checked_add(size_amount) {
            None => return Err(PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW)),
            Some(new_size) => {
                if new_size > self.stack_size_high_water_mark {
                    self.stack_size_high_water_mark = new_size;
                }
                self.stack_size_current = new_size;
            }
        }

        if let Some(stack_size_tier_next) = self.stack_size_next_tier_start
            && self.stack_size_current > stack_size_tier_next
        {
            let (next_mul, next_tier) = self.cost_table.stack_size_tier(self.stack_size_current);
            self.stack_size_current_tier_mult = next_mul;
            self.stack_size_next_tier_start = next_tier;
        }

        Ok(())
    }

    pub fn decrease_stack_size(&mut self, size_amount: u64) {
        let new_size = self.stack_size_current.saturating_sub(size_amount);
        if new_size > self.stack_size_high_water_mark {
            self.stack_size_high_water_mark = new_size;
        }
        self.stack_size_current = new_size;
    }

    /// Given: pushes + pops + increase + decrease in size for an instruction charge for the
    /// execution of the instruction.
    pub fn charge(
        &mut self,
        num_instructions: u64,
        pushes: u64,
        pops: u64,
        incr_size: u64,
        _decr_size: u64,
    ) -> PartialVMResult<()> {
        self.push_stack(pushes)?;
        self.increase_instruction_count(num_instructions)?;
        self.increase_stack_size(incr_size)?;

        self.deduct_gas(
            GasCost::new(
                self.instructions_current_tier_mult
                    .checked_mul(num_instructions)
                    .ok_or_else(|| PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW))?,
                self.stack_size_current_tier_mult
                    .checked_mul(incr_size)
                    .ok_or_else(|| PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW))?,
                self.stack_height_current_tier_mult
                    .checked_mul(pushes)
                    .ok_or_else(|| PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW))?,
            )
            .total_internal(),
        )?;

        // self.decrease_stack_size(decr_size);
        self.pop_stack(pops);
        Ok(())
    }

    /// Return the `CostTable` behind this `GasStatus`.
    pub fn cost_table(&self) -> &CostTable {
        &self.cost_table
    }

    /// Return the gas left.
    pub fn remaining_gas(&self) -> Gas {
        self.gas_left.to_unit_round_down()
    }

    /// Charge a given amount of gas and fail if not enough gas units are left.
    pub fn deduct_gas(&mut self, amount: InternalGas) -> PartialVMResult<()> {
        if !self.charge {
            return Ok(());
        }

        match self.gas_left.checked_sub(amount) {
            Some(gas_left) => {
                self.gas_left = gas_left;
                Ok(())
            }
            None => {
                self.gas_left = InternalGas::new(0);
                Err(PartialVMError::new(StatusCode::OUT_OF_GAS))
            }
        }
    }

    pub fn record_native_call(&mut self) {
        self.num_native_calls = self.num_native_calls.saturating_add(1);
    }

    // Deduct the amount provided with no conversion, as if it was InternalGasUnit
    fn deduct_units(&mut self, amount: u64) -> PartialVMResult<()> {
        self.deduct_gas(InternalGas::new(amount))
    }

    pub fn set_metering(&mut self, enabled: bool) {
        self.charge = enabled
    }

    // The amount of gas used, it does not include the multiplication for the gas price
    pub fn gas_used_pre_gas_price(&self) -> u64 {
        let gas: Gas = match self.initial_budget.checked_sub(self.gas_left) {
            Some(val) => InternalGas::to_unit_round_down(val),
            None => InternalGas::to_unit_round_down(self.initial_budget),
        };
        u64::from(gas)
    }

    // Charge the number of bytes with the cost per byte value
    // As more bytes are read throughout the computation the cost per bytes is increased.
    pub fn charge_bytes(&mut self, size: usize, cost_per_byte: u64) -> PartialVMResult<()> {
        let computation_cost = if charge_input_as_memory(self.gas_model_version) {
            self.increase_stack_size(size as u64)?;
            self.stack_size_current_tier_mult * size as u64 * cost_per_byte
        } else {
            size as u64 * cost_per_byte
        };
        self.deduct_units(computation_cost)
    }

    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    pub fn stack_height_high_water_mark(&self) -> u64 {
        self.stack_height_high_water_mark
    }

    pub fn stack_size_high_water_mark(&self) -> u64 {
        self.stack_size_high_water_mark
    }

    pub fn instructions_executed(&self) -> u64 {
        self.instructions_executed
    }
}

pub fn zero_cost_schedule() -> CostTable {
    let mut zero_tier = BTreeMap::new();
    zero_tier.insert(0, 0);
    CostTable {
        instruction_tiers: zero_tier.clone(),
        stack_size_tiers: zero_tier.clone(),
        stack_height_tiers: zero_tier,
    }
}

pub fn unit_cost_schedule() -> CostTable {
    let mut unit_tier = BTreeMap::new();
    unit_tier.insert(0, 1);
    CostTable {
        instruction_tiers: unit_tier.clone(),
        stack_size_tiers: unit_tier.clone(),
        stack_height_tiers: unit_tier,
    }
}

pub fn initial_cost_schedule_v1() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (3000, 2),
        (6000, 3),
        (8000, 5),
        (9000, 9),
        (9500, 16),
        (10000, 29),
        (10500, 50),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (400, 2),
        (800, 3),
        (1200, 5),
        (1500, 9),
        (1800, 16),
        (2000, 29),
        (2200, 50),
    ]
    .into_iter()
    .collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (2000, 2),
        (5000, 3),
        (8000, 5),
        (10000, 9),
        (11000, 16),
        (11500, 29),
        (11500, 50),
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

pub fn initial_cost_schedule_v2() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (3000, 2),
        (6000, 3),
        (8000, 5),
        (9000, 9),
        (9500, 16),
        (10000, 29),
        (10500, 50),
        (12000, 150),
        (15000, 250),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (400, 2),
        (800, 3),
        (1200, 5),
        (1500, 9),
        (1800, 16),
        (2000, 29),
        (2200, 50),
        (3000, 150),
        (5000, 250),
    ]
    .into_iter()
    .collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (2000, 2),
        (5000, 3),
        (8000, 5),
        (10000, 9),
        (11000, 16),
        (11500, 29),
        (11500, 50),
        (15000, 150),
        (20000, 250),
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

pub fn initial_cost_schedule_v3() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (3000, 2),
        (6000, 3),
        (8000, 5),
        (9000, 9),
        (9500, 16),
        (10000, 29),
        (10500, 50),
        (15000, 100),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (400, 2),
        (800, 3),
        (1200, 5),
        (1500, 9),
        (1800, 16),
        (2000, 29),
        (2200, 50),
        (5000, 100),
    ]
    .into_iter()
    .collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (2000, 2),
        (5000, 3),
        (8000, 5),
        (10000, 9),
        (11000, 16),
        (11500, 29),
        (11500, 50),
        (20000, 100),
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

pub fn initial_cost_schedule_v4() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (20_000, 2),
        (50_000, 10),
        (100_000, 50),
        (200_000, 100),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> =
        vec![(0, 1), (1_000, 2), (10_000, 10)].into_iter().collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (100_000, 2),     // ~100K
        (500_000, 5),     // ~500K
        (1_000_000, 100), // ~1M
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

pub fn initial_cost_schedule_v5() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (20_000, 2),
        (50_000, 10),
        (100_000, 50),
        (200_000, 100),
        (10_000_000, 1000),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> =
        vec![(0, 1), (1_000, 2), (10_000, 10)].into_iter().collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (100_000, 2),        // ~100K
        (500_000, 5),        // ~500K
        (1_000_000, 100),    // ~1M
        (100_000_000, 1000), // ~100M
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

// Convert from our representation of gas costs to the type that the MoveVM expects for unit tests.
// We don't want our gas depending on the MoveVM test utils and we don't want to fix our
// representation to whatever is there, so instead we perform this translation from our gas units
// and cost schedule to the one expected by the Move unit tests.
pub fn initial_cost_schedule_for_unit_tests() -> move_vm_test_utils::gas_schedule::CostTable {
    let table = initial_cost_schedule_v5();
    move_vm_test_utils::gas_schedule::CostTable {
        instruction_tiers: table.instruction_tiers.into_iter().collect(),
        stack_height_tiers: table.stack_height_tiers.into_iter().collect(),
        stack_size_tiers: table.stack_size_tiers.into_iter().collect(),
    }
}
