// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Add;

use move_core_types::gas_algebra::{GasQuantity, InternalGasUnit, ToUnit, ToUnitFractional};
use serde::{Deserialize, Serialize};

pub enum GasUnit {}

pub type Gas = GasQuantity<GasUnit>;

impl ToUnit<InternalGasUnit> for GasUnit {
    const MULTIPLIER: u64 = 1000;
}

impl ToUnitFractional<GasUnit> for InternalGasUnit {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1000;
}

/// The cost tables, keyed by the serialized form of the bytecode instruction.  We use the
/// serialized form as opposed to the instruction enum itself as the key since this will be the
/// on-chain representation of bytecode instructions in the future.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Deserialize)]
pub struct CostTable {
    pub instruction_table: Vec<GasCost>,
}

impl CostTable {
    #[inline]
    pub fn instruction_cost(&self, instr_index: u8) -> &GasCost {
        debug_assert!(instr_index > 0 && instr_index <= (self.instruction_table.len() as u8));
        &self.instruction_table[(instr_index - 1) as usize]
    }
}

/// The  `GasCost` tracks:
/// - instruction cost: how much time/computational power is needed to perform the instruction
/// - memory cost: how much memory is required for the instruction, and storage overhead
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GasCost {
    pub instruction_gas: u64,
    pub memory_gas: u64,
}

impl GasCost {
    pub fn new(instruction_gas: u64, memory_gas: u64) -> Self {
        Self {
            instruction_gas,
            memory_gas,
        }
    }

    /// Convert a GasCost to a total gas charge in `InternalGas`.
    #[inline]
    pub fn total(&self) -> u64 {
        self.instruction_gas.add(self.memory_gas)
    }
}
