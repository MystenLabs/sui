// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMError;
use move_core_types::gas_algebra::InternalGas;
use move_core_types::vm_status::StatusCode;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::ObjectID,
    error::ExecutionError,
    execution_status::ExecutionErrorKind,
    gas::{GasCostSummary, GasUsageReport},
    gas_model::{
        gas_common::{PerObjectStorage, package_read_internal_gas, sender_rebate},
        gas_predicates::cost_table_for_version,
        tables::GasStatus,
        units_types::{CostTable, Gas},
    },
};

// MIST amounts, and prices in MIST per gas unit.
type Mist = u64;
// Gas units, as the transaction is charged.
type GasUnits = u64;
// Internal gas units, as the Move gas status counts them.
type InternalUnits = u64;

// How a transaction is metered
#[derive(Debug, Clone, Copy)]
pub(crate) enum Metering {
    Unmetered,
    Metered {
        budget: Mist,
        price: Mist,
        reference_gas_price: Mist,
    },
}

impl Metering {
    fn is_unmetered(&self) -> bool {
        matches!(self, Metering::Unmetered)
    }

    fn budget(&self) -> Mist {
        match self {
            Metering::Unmetered => 0,
            Metering::Metered { budget, .. } => *budget,
        }
    }

    fn price(&self) -> Mist {
        match self {
            Metering::Unmetered => 0,
            Metering::Metered { price, .. } => *price,
        }
    }

    fn reference_gas_price(&self) -> Mist {
        match self {
            Metering::Unmetered => 0,
            Metering::Metered {
                reference_gas_price,
                ..
            } => *reference_gas_price,
        }
    }
}

// Holds protocol config values and configuration/cost data for gas charging
#[derive(Debug)]
struct GasConfig {
    gas_model_version: u64,
    cost_table: CostTable,
    // Internal gas units per byte of input object read.
    read_cost_per_byte: InternalUnits,
    // Internal gas units per KiB of package read. Packages use `read_cost_per_byte` when
    // unset.
    package_read_per_kb_cost: Option<InternalUnits>,
    // Internal gas units per byte of published module.
    publish_cost_per_byte: InternalUnits,
    // Cap, in gas units, on the execution meter budget.
    max_computation_bucket: GasUnits,
    // Minimum computation charge in gas units: the base transaction cost.
    min_computation_units: GasUnits,
    storage_gas_price: Mist,
    storage_per_byte_cost: GasUnits,
    rebate_rate: u64,
    // RGP multiplier capping the effective gas price of aborted transactions.
    abort_price_factor: Option<u64>,
}

impl GasConfig {
    fn new(config: &ProtocolConfig) -> Self {
        Self {
            gas_model_version: config.gas_model_version(),
            cost_table: cost_table_for_version(config.gas_model_version()),
            read_cost_per_byte: config.obj_access_cost_read_per_byte(),
            package_read_per_kb_cost: config.obj_access_cost_read_per_package_kb_as_option(),
            publish_cost_per_byte: config.package_publish_cost_per_byte(),
            max_computation_bucket: config.max_gas_computation_bucket(),
            min_computation_units: config.base_tx_cost_fixed(),
            storage_gas_price: config.storage_gas_price(),
            storage_per_byte_cost: config.obj_data_cost_refundable(),
            rebate_rate: config.storage_rebate_rate(),
            abort_price_factor: config
                .max_gas_price_rgp_factor_for_aborted_transactions_as_option(),
        }
    }
}

// Gas state of a transaction, in MIST. The phases are pre-execution, execution (input
// objects and the PTB) and storage. Each phase settles once against the budget in MIST.
// A computation phase that does not fit marks the state out of gas. A storage
// tier that does not fit leaves the state unchanged.
#[derive(Debug)]
pub(crate) struct GasState {
    metering: Metering,
    config: GasConfig,
    // MIST owed before the VM runs.
    pre_execution: Mist,
    computation: Computation,
    storage: StorageCharge,
    // A computation check failed, or storage settled at `RebatesOnly`.
    out_of_gas: bool,
}

// The units the execution meter used, input objects included, and the price fixed when
// execution settles.
#[derive(Debug, Default)]
struct Computation {
    units: InternalUnits,
    settled_price: Option<Mist>,
}

impl Computation {
    fn mist(&self, price: Mist, min_units: GasUnits) -> Mist {
        let units = half_digits_rounding(to_gas_units(self.units)).max(min_units);
        units.saturating_mul(price)
    }
}

// The write set storage settles on. Tiers only advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StorageTier {
    // Every write of the transaction.
    Full,
    // The bumped mutable inputs and the re-smashed gas.
    InputsOnly,
    // The rebates of the smashed coins, with the whole budget reported as computation.
    RebatesOnly,
}

impl GasState {
    pub fn new(metering: Metering, config: &ProtocolConfig) -> Self {
        Self {
            metering,
            config: GasConfig::new(config),
            pre_execution: 0,
            computation: Computation::default(),
            storage: StorageCharge::default(),
            out_of_gas: false,
        }
    }

    pub fn is_unmetered(&self) -> bool {
        self.metering.is_unmetered()
    }

    pub fn budget(&self) -> Mist {
        self.metering.budget()
    }

    pub fn price(&self) -> Mist {
        self.metering.price()
    }

    pub fn reference_gas_price(&self) -> Mist {
        self.metering.reference_gas_price()
    }

    // MIST charged so far: pre-execution plus computation. Computation counts only once
    // execution settles.
    fn charged(&self) -> Mist {
        let computation = self.computation.settled_price.map_or(0, |price| {
            self.computation
                .mist(price, self.config.min_computation_units)
        });
        self.pre_execution.saturating_add(computation)
    }

    // First phase, just the budget
    fn pre_execution_limit(&self) -> Mist {
        self.budget()
    }

    // budget - pre_execution
    fn execution_limit(&self) -> Mist {
        self.budget().saturating_sub(self.pre_execution)
    }

    // budget - computation - pre_execution
    fn storage_limit(&self) -> Mist {
        self.budget().saturating_sub(self.charged())
    }

    // `charge` must stay below `limit`, else out of gas.
    fn check_below_limit(&mut self, charge: Mist, limit: Mist) -> Result<(), ExecutionError> {
        if !self.is_unmetered() && charge >= limit {
            self.out_of_gas = true;
            return Err(ExecutionErrorKind::InsufficientGas.into());
        }
        Ok(())
    }

    // Pre execution phase.

    pub fn settle_pre_execution(&mut self, mist: Mist) -> Result<(), ExecutionError> {
        self.pre_execution = mist;
        self.check_below_limit(self.pre_execution, self.pre_execution_limit())
    }

    // Execution phase.

    // Open the execution phase. The meter budget is the execution limit, capped by the
    // computation bucket.
    pub fn execution_meter(&mut self) -> Result<ExecutionMeter, ExecutionError> {
        assert_invariant!(
            self.computation.settled_price.is_none(),
            "the execution phase opens before it settles"
        );
        let status = match self.metering {
            Metering::Metered { price, .. } => {
                let cap = self.config.max_computation_bucket.saturating_mul(price);
                GasStatus::new(
                    self.config.cost_table.clone(),
                    self.execution_limit().min(cap),
                    price,
                    self.config.gas_model_version,
                )
            }
            Metering::Unmetered => GasStatus::new_unmetered(),
        };
        Ok(ExecutionMeter::new(
            status,
            InputRates {
                cost_per_byte: self.config.read_cost_per_byte,
                package_cost_per_kb: self.config.package_read_per_kb_cost,
            },
            self.config.publish_cost_per_byte,
        ))
    }

    // Close the execution phase at the signed price, or at the capped price on abort.
    pub fn settle_execution(
        &mut self,
        meter: ExecutionMeter,
        aborted: bool,
    ) -> Result<(), ExecutionError> {
        assert_invariant!(
            self.computation.settled_price.is_none(),
            "the execution phase settles once"
        );
        let price = match (self.metering, self.config.abort_price_factor) {
            (Metering::Unmetered, _) => 0,
            (
                Metering::Metered {
                    price,
                    reference_gas_price,
                    ..
                },
                Some(factor),
            ) if aborted => price.min(factor.saturating_mul(reference_gas_price)),
            (Metering::Metered { price, .. }, _) => price,
        };
        self.computation.units = meter.used_internal_units();
        self.computation.settled_price = Some(price);
        let computation = self
            .computation
            .mist(price, self.config.min_computation_units);
        self.check_below_limit(computation, self.execution_limit())
    }

    // Storage phase.

    pub fn storage_rates(&self) -> StorageRates {
        if self.is_unmetered() {
            StorageRates::unmetered()
        } else {
            StorageRates {
                price: self.config.storage_gas_price,
                per_byte_cost: self.config.storage_per_byte_cost,
                metered: true,
            }
        }
    }

    // `Full` and `InputsOnly` must fit net of the sender's rebate, and leave the state
    // unchanged when they do not. `RebatesOnly` always settles and runs the budget out.
    pub fn settle_storage(
        &mut self,
        charge: StorageCharge,
        tier: StorageTier,
    ) -> Result<(), ExecutionError> {
        match tier {
            StorageTier::RebatesOnly => self.out_of_gas = true,
            StorageTier::Full | StorageTier::InputsOnly => {
                let net_cost = charge
                    .cost
                    .saturating_sub(sender_rebate(charge.rebate, self.config.rebate_rate));
                if !self.is_unmetered() && net_cost > self.storage_limit() {
                    return Err(ExecutionErrorKind::InsufficientGas.into());
                }
            }
        }
        self.storage = charge;
        Ok(())
    }

    // Reporting.

    // Computation is the whole budget once out of gas. Storage settled only if it fit, so
    // the net charge never exceeds the budget.
    pub fn summary(&self) -> GasCostSummary {
        let storage_rebate = self.storage.rebate;
        let sender_rebate = sender_rebate(storage_rebate, self.config.rebate_rate);
        let non_refundable_storage_fee = storage_rebate
            .checked_sub(sender_rebate)
            .expect("sender rebate must not exceed storage rebate");
        let storage_cost = self.storage.cost;
        let computation_cost = if self.out_of_gas {
            self.budget()
        } else {
            self.charged()
        };
        debug_assert!(
            self.out_of_gas
                || self.is_unmetered()
                || computation_cost.saturating_add(storage_cost.saturating_sub(sender_rebate))
                    <= self.budget(),
            "storage must fit in what computation left"
        );
        GasCostSummary {
            computation_cost,
            storage_cost,
            storage_rebate: sender_rebate,
            non_refundable_storage_fee,
        }
    }

    pub fn gas_usage_report(&self) -> GasUsageReport {
        GasUsageReport {
            cost_summary: self.summary(),
            gas_used: to_gas_units(self.computation.units),
            gas_budget: self.budget(),
            gas_price: self.price(),
            reference_gas_price: self.reference_gas_price(),
            storage_gas_price: if self.is_unmetered() {
                0
            } else {
                self.config.storage_gas_price
            },
            rebate_rate: self.config.rebate_rate,
            per_object_storage: self.storage.per_object.clone(),
        }
    }

    pub fn unmetered_storage_rebate(&self) -> Mist {
        self.storage.unmetered_rebate
    }
}

// Rounds down, as the Move meter does.
fn to_gas_units(internal_units: InternalUnits) -> GasUnits {
    let units: Gas = InternalGas::new(internal_units).to_unit_round_down();
    u64::from(units)
}

// Round up to half the digits: 1234 becomes 1240, 123456 becomes 123500.
fn half_digits_rounding(n: u64) -> u64 {
    if n < 10 {
        return n;
    }
    let digits = n.ilog10();
    let drop = digits / 2;
    let base = 10u64.pow(drop);
    n.div_ceil(base).saturating_mul(base)
}

// Rates of the input objects, in internal gas units.
#[derive(Debug, Clone, Copy)]
struct InputRates {
    cost_per_byte: InternalUnits,
    package_cost_per_kb: Option<InternalUnits>,
}

impl InputRates {
    fn units(&self, size: usize, is_package: bool) -> InternalUnits {
        match (is_package, self.package_cost_per_kb) {
            (true, Some(cost_per_kb)) => package_read_internal_gas(size, cost_per_kb),
            _ => (size as u64).saturating_mul(self.cost_per_byte),
        }
    }
}

// The meter of the execution phase: the Move gas status, with byte charges for input objects
// and the PTB.
#[derive(Debug)]
pub(crate) struct ExecutionMeter {
    status: GasStatus,
    initial_internal_units: InternalUnits,
    input_rates: InputRates,
    publish_cost_per_byte: InternalUnits,
}

impl ExecutionMeter {
    fn new(
        status: GasStatus,
        input_rates: InputRates,
        publish_cost_per_byte: InternalUnits,
    ) -> Self {
        // A fresh meter has its whole budget left.
        let initial_internal_units = u64::from(status.gas_left);
        Self {
            status,
            initial_internal_units,
            input_rates,
            publish_cost_per_byte,
        }
    }

    // A meter that charges nothing, for genesis.
    pub fn unmetered(config: &ProtocolConfig) -> Self {
        Self::new(
            GasStatus::new_unmetered(),
            InputRates {
                cost_per_byte: config.obj_access_cost_read_per_byte(),
                package_cost_per_kb: config.obj_access_cost_read_per_package_kb_as_option(),
            },
            config.package_publish_cost_per_byte(),
        )
    }

    pub fn status(&self) -> &GasStatus {
        &self.status
    }

    pub fn status_mut(&mut self) -> &mut GasStatus {
        &mut self.status
    }

    fn used_internal_units(&self) -> InternalUnits {
        self.initial_internal_units
            .saturating_sub(u64::from(self.status.gas_left))
    }

    pub fn charge_input_object(
        &mut self,
        size: usize,
        is_package: bool,
    ) -> Result<(), ExecutionError> {
        let units = self.input_rates.units(size, is_package);
        self.status
            .deduct_gas(InternalGas::new(units))
            .map_err(out_of_gas)
    }

    pub fn charge_publish_package(&mut self, size: usize) -> Result<(), ExecutionError> {
        self.charge_bytes(size, self.publish_cost_per_byte)
    }

    pub fn charge_read_bytes(&mut self, size: usize) -> Result<(), ExecutionError> {
        self.charge_bytes(size, self.input_rates.cost_per_byte)
    }

    fn charge_bytes(
        &mut self,
        size: usize,
        cost_per_byte: InternalUnits,
    ) -> Result<(), ExecutionError> {
        self.status
            .charge_bytes(size, cost_per_byte)
            .map_err(out_of_gas)
    }
}

fn out_of_gas(e: PartialVMError) -> ExecutionError {
    debug_assert_eq!(e.major_status(), StatusCode::OUT_OF_GAS);
    ExecutionErrorKind::InsufficientGas.into()
}

// Rates of the storage phase. Unmetered transactions track only their rebate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StorageRates {
    price: Mist,
    per_byte_cost: GasUnits,
    metered: bool,
}

impl StorageRates {
    fn unmetered() -> Self {
        Self {
            price: 0,
            per_byte_cost: 0,
            metered: false,
        }
    }
}

// Storage cost and rebate of a write set, with the per-object breakdown. The store builds
// it and the state settles it. `unmetered_rebate` is the rebate of an unmetered
// transaction, which the charger moves to the system state object.
#[derive(Debug, Default)]
pub(crate) struct StorageCharge {
    cost: Mist,
    rebate: Mist,
    unmetered_rebate: Mist,
    per_object: Vec<(ObjectID, PerObjectStorage)>,
}

impl StorageCharge {
    // Track a written object, or a deleted one with `new_size` zero. Returns the new storage
    // rebate, or `None` on overflow.
    pub fn track_mutation(
        &mut self,
        rates: &StorageRates,
        object_id: ObjectID,
        new_size: usize,
        storage_rebate: Mist,
    ) -> Option<Mist> {
        if !rates.metered {
            self.unmetered_rebate = self.unmetered_rebate.checked_add(storage_rebate)?;
            return Some(0);
        }
        let new_size = new_size as u64;
        let storage_cost = new_size
            .checked_mul(rates.per_byte_cost)?
            .checked_mul(rates.price)?;
        self.cost = self.cost.checked_add(storage_cost)?;
        self.rebate = self.rebate.checked_add(storage_rebate)?;
        self.per_object.push((
            object_id,
            PerObjectStorage {
                storage_cost,
                storage_rebate,
                new_size,
            },
        ));
        Some(storage_cost)
    }
}
