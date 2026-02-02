// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_vm_config::runtime::VMConfig;
use move_vm_runtime::native_functions::NativeFunctionTable;
use move_vm_test_utils::gas_schedule::{Gas, GasStatus, unit_cost_schedule};
use move_vm_types::gas::GasMeter;

pub trait VMTestSetup {
    type Meter<'a>: GasMeter + Send
    where
        Self: 'a;
    fn new_meter<'a>(&'a self, execution_bound: Option<u64>) -> Self::Meter<'a>;
    fn used_gas<'a>(&'a self, execution_bound: u64, meter: Self::Meter<'a>) -> u64;
    fn vm_config(&self) -> VMConfig;
    fn native_function_table(&self) -> NativeFunctionTable;
}

pub struct DefaultVMTestSetup {
    pub cost_table: move_vm_test_utils::gas_schedule::CostTable,
    pub native_function_table: NativeFunctionTable,
}

impl DefaultVMTestSetup {
    pub fn new(
        cost_table: move_vm_test_utils::gas_schedule::CostTable,
        native_function_table: NativeFunctionTable,
    ) -> Self {
        Self {
            cost_table,
            native_function_table,
        }
    }

    pub fn legacy_default() -> Self {
        Self::new(
            unit_cost_schedule(),
            move_stdlib_natives::all_natives(
                AccountAddress::ONE,
                move_stdlib_natives::GasParameters::zeros(),
                /* silent */ false,
            ),
        )
    }
}

impl VMTestSetup for DefaultVMTestSetup {
    type Meter<'a> = GasStatus<'a>;

    fn new_meter(&self, execution_bound: Option<u64>) -> Self::Meter<'_> {
        if let Some(bound) = execution_bound {
            GasStatus::new(&self.cost_table, Gas::new(bound))
        } else {
            GasStatus::new_unmetered()
        }
    }

    fn used_gas(&self, execution_bound: u64, meter: Self::Meter<'_>) -> u64 {
        // TODO(Gas): This doesn't look quite right...
        //            We're not computing the number of instructions executed even with a unit gas schedule.
        Gas::new(execution_bound)
            .checked_sub(meter.remaining_gas())
            .unwrap()
            .into()
    }

    fn vm_config(&self) -> VMConfig {
        VMConfig::default()
    }

    fn native_function_table(&self) -> NativeFunctionTable {
        self.native_function_table.clone()
    }
}
