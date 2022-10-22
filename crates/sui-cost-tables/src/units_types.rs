// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Mul;

use anyhow::anyhow;
use move_core_types::gas_algebra::{
    GasQuantity, InternalGasUnit, ToUnit, ToUnitFractional, UnitDiv,
};

pub enum GasUnit {}

pub type Gas = GasQuantity<GasUnit>;

impl ToUnit<InternalGasUnit> for GasUnit {
    const MULTIPLIER: u64 = 1000;
}

impl ToUnitFractional<GasUnit> for InternalGasUnit {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1000;
}

/// Linear equation for: Y = Mx + C
/// For example when calculating the price for publishing a package,
/// we may want to price per byte, with some offset
/// Hence: cost = package_cost_per_byte * num_bytes + base_cost
/// For consistency, the units must be defined as UNIT(package_cost_per_byte) = UnitDiv(UNIT(cost), UNIT(num_bytes))
pub struct LinearEquation<YUnit, XUnit> {
    offset: GasQuantity<YUnit>,
    slope: GasQuantity<UnitDiv<YUnit, XUnit>>,
    min: GasQuantity<YUnit>,
    max: GasQuantity<YUnit>,
}

impl<YUnit, XUnit> LinearEquation<YUnit, XUnit> {
    pub const fn new(
        slope: GasQuantity<UnitDiv<YUnit, XUnit>>,
        offset: GasQuantity<YUnit>,
        min: GasQuantity<YUnit>,
        max: GasQuantity<YUnit>,
    ) -> Self {
        Self {
            offset,
            slope,
            min,
            max,
        }
    }
    #[inline]
    pub fn calculate(&self, x: GasQuantity<XUnit>) -> anyhow::Result<GasQuantity<YUnit>> {
        let y = self.offset + self.slope.mul(x);

        if y < self.min {
            Err(anyhow!(
                "Value {} is below minumum allowed {}",
                u64::from(y),
                u64::from(self.min)
            ))
        } else if y > self.max {
            Err(anyhow!(
                "Value {} is above maximum allowed {}",
                u64::from(y),
                u64::from(self.max)
            ))
        } else {
            Ok(y)
        }
    }
}
