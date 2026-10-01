// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(clippy::indexing_slicing)]
#![deny(clippy::cast_possible_wrap)]
#![deny(clippy::cast_sign_loss)]

use crate::error::{UserInputError, UserInputResult};
use crate::transaction::ObjectReadResult;
use crate::{ObjectID, gas};
use serde::{Deserialize, Serialize};

pub fn check_gas_objects(gas_objs: &[&ObjectReadResult]) -> UserInputResult {
    // All gas objects have an address owner
    // Note: because of address balance payments, gas_objs may be empty.
    for gas_object in gas_objs {
        // if as_object() returns None, it means the object has been deleted (and therefore
        // must be a shared object).
        if let Some(obj) = gas_object.as_object() {
            if !obj.is_address_owned() {
                return Err(UserInputError::GasObjectNotOwnedObject {
                    owner: obj.owner.clone(),
                });
            }
        } else {
            // This case should never happen (because gas can't be a shared object), but we
            // handle this case for future-proofing
            return Err(UserInputError::MissingGasPayment);
        }
    }
    Ok(())
}

pub fn check_gas_data(
    gas_objs: &[&ObjectReadResult],
    gas_budget: u64,
    available_address_balance_gas: u64,
    min_transaction_cost: u64,
    max_gas_budget: u64,
) -> UserInputResult {
    // Gas budget is between min and max budget allowed
    if gas_budget > max_gas_budget {
        return Err(UserInputError::GasBudgetTooHigh {
            gas_budget,
            max_budget: max_gas_budget,
        });
    }
    if gas_budget < min_transaction_cost {
        return Err(UserInputError::GasBudgetTooLow {
            gas_budget,
            min_budget: min_transaction_cost,
        });
    }

    // Gas balance (all gas coins + address balance together) is bigger or equal to budget
    let mut gas_balance = available_address_balance_gas as u128;
    for gas_obj in gas_objs {
        // Saturation is unreachable: a sum of u64 coin balances cannot overflow u128.
        gas_balance = gas_balance.saturating_add(gas::get_gas_balance(gas_obj.as_object().ok_or(
            UserInputError::InvalidGasObject {
                object_id: gas_obj.id(),
            },
        )?)? as u128);
    }
    if gas_balance < gas_budget as u128 {
        Err(UserInputError::GasBalanceTooLow {
            gas_balance,
            needed_gas_amount: gas_budget as u128,
        })
    } else {
        Ok(())
    }
}

/// Portion of the storage rebate that gets passed on to the transaction sender. The remainder
/// will be burned, then re-minted + added to the storage fund at the next epoch change
pub fn sender_rebate(storage_rebate: u64, storage_rebate_rate: u64) -> u64 {
    // we round storage rebate such that `>= x.5` goes to x+1 (rounds up) and
    // `< x.5` goes to x (truncates). We replicate `f32/64::round()`
    const BASIS_POINTS: u128 = 10000;
    let rebate = (storage_rebate as u128)
        .saturating_mul(storage_rebate_rate as u128)
        .saturating_add(BASIS_POINTS / 2) // integer rounding adds half of the denominator
        / BASIS_POINTS;
    u64::try_from(rebate).unwrap_or(u64::MAX)
}

/// Internal gas for reading `size` bytes of package inputs at `cost_per_kb` per 1,000 bytes, rounded
/// up. Saturates at `u64::MAX` instead of overflowing.
pub fn package_read_internal_gas(size: usize, cost_per_kb: u64) -> u64 {
    // The product of two values below 2^64 always fits in a u128.
    let cost = (size as u128)
        .saturating_mul(u128::from(cost_per_kb))
        .div_ceil(1000);
    u64::try_from(cost).unwrap_or(u64::MAX)
}

pub fn half_digits_rounding(n: u64) -> u64 {
    if n < 1000 {
        return 1000;
    }
    let digits = n.ilog10();
    let drop = digits / 2;
    let base = 10u64.pow(drop);
    n.div_ceil(base).saturating_mul(base)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerObjectStorage {
    /// The new "value" for this object storage. Computed
    /// at the end of execution while determining storage charges.
    /// This will be the new storage rebate.
    pub storage_cost: u64,
    /// storage_rebate is the value of this object.
    /// This is computed at the end of execution while determining storage charges.
    /// The value is in Sui.
    pub storage_rebate: u64,
    /// The object size post-transaction in bytes
    pub new_size: u64,
}

/// Per-object storage-gas accumulator, shared by both gas models. Pure data + arithmetic; the Move
/// meter stays on the outer `SuiGasStatus`, which passes the `unmetered` flag into `track_mutation`.
#[derive(Debug)]
pub struct StorageGas {
    /// Per-object storage cost + rebate, accumulated during execution.
    per_object_storage: Vec<(ObjectID, PerObjectStorage)>,
    /// Running total of per-object storage cost. Metered path only.
    total_storage_cost: u64,
    /// Running total of per-object storage rebate. Metered path only.
    total_storage_rebate: u64,
    /// Storage rebate accrued while running unmetered (system transactions), retained in effects
    /// and parked onto 0x5. Kept separate from `total_storage_rebate`: it must read 0 on metered
    /// txns (its consumer `conserve_unmetered_storage_rebate` runs unconditionally).
    unmetered_storage_rebate: u64,
    /// Multiplier applied to the storage byte cost (`ProtocolConfig::storage_gas_price`).
    pub storage_gas_price: u64,
    /// Refundable per-byte storage cost (`ProtocolConfig::obj_data_cost_refundable`).
    storage_per_byte_cost: u64,
}

impl StorageGas {
    pub fn new(storage_gas_price: u64, storage_per_byte_cost: u64) -> Self {
        Self {
            per_object_storage: Vec::new(),
            total_storage_cost: 0,
            total_storage_rebate: 0,
            unmetered_storage_rebate: 0,
            storage_gas_price,
            storage_per_byte_cost,
        }
    }

    pub fn storage_gas_units(&self) -> u64 {
        self.total_storage_cost
    }

    pub fn storage_rebate(&self) -> u64 {
        self.total_storage_rebate
    }

    pub fn unmetered_storage_rebate(&self) -> u64 {
        self.unmetered_storage_rebate
    }

    pub fn per_object_storage(&self) -> &Vec<(ObjectID, PerObjectStorage)> {
        &self.per_object_storage
    }

    pub fn reset(&mut self) {
        self.per_object_storage = Vec::new();
        self.total_storage_cost = 0;
        self.total_storage_rebate = 0;
        self.unmetered_storage_rebate = 0;
    }

    /// Update the running storage cost/rebate totals for the object.
    /// Returns the new object storage cost (based on `new_size`), or `None` on overflow.
    pub fn track_mutation(
        &mut self,
        object_id: ObjectID,
        new_size: usize,
        storage_rebate: u64,
        unmetered: bool,
    ) -> Option<u64> {
        if unmetered {
            let total = self.unmetered_storage_rebate.checked_add(storage_rebate)?;
            self.unmetered_storage_rebate = total;
            return Some(0);
        }

        let new_size = new_size as u64;
        let storage_cost = new_size
            .checked_mul(self.storage_per_byte_cost)?
            .checked_mul(self.storage_gas_price)?;
        self.total_storage_cost = self.total_storage_cost.checked_add(storage_cost)?;
        self.total_storage_rebate = self.total_storage_rebate.checked_add(storage_rebate)?;
        self.per_object_storage.push((
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

#[test]
fn test_half_digits_rounding() {
    assert_eq!(half_digits_rounding(0), 1000);
    assert_eq!(half_digits_rounding(1), 1000);
    assert_eq!(half_digits_rounding(999), 1000);
    assert_eq!(half_digits_rounding(1000), 1000);
    assert_eq!(half_digits_rounding(1001), 1010);
    assert_eq!(half_digits_rounding(1050), 1050);
    assert_eq!(half_digits_rounding(1999), 2000);
    assert_eq!(half_digits_rounding(20_000), 20_000);
    assert_eq!(half_digits_rounding(20_001), 20_100);
    assert_eq!(half_digits_rounding(20_500), 20_500);
    assert_eq!(half_digits_rounding(29_999), 30_000);
    assert_eq!(half_digits_rounding(300_000), 300_000);
    assert_eq!(half_digits_rounding(300_001), 300_100);
    assert_eq!(half_digits_rounding(305_500), 305_500);
    assert_eq!(half_digits_rounding(305_501), 305_600);
    assert_eq!(half_digits_rounding(999_999), 1_000_000);
    assert_eq!(half_digits_rounding(1_000_000), 1_000_000);
    assert_eq!(half_digits_rounding(1_000_001), 1_001_000);
    assert_eq!(half_digits_rounding(1_005_000), 1_005_000);
    assert_eq!(half_digits_rounding(1_005_001), 1_006_000);
    assert_eq!(half_digits_rounding(1_999_999), 2_000_000);
    assert_eq!(half_digits_rounding(10_000_001), 10_001_000);
    assert_eq!(half_digits_rounding(100_000_001), 100_010_000);
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod package_read_tests {
    use super::*;
    use crate::error::ExecutionError;
    use crate::gas::{SuiGasStatus, SuiGasStatusAPI};
    use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

    const RGP: u64 = 1_000;

    // Sizes around the 1,000-byte rounding unit and the package and object size limits.
    const SIZES: &[usize] = &[
        0,
        1,
        2,
        6,
        7,
        199,
        200,
        999,
        1_000,
        1_001,
        1_999,
        89_315,
        100 * 1024,
        250 * 1024 + 89,
    ];

    #[test]
    fn package_read_rounds_up() {
        let charge = |size| package_read_internal_gas(size, 150);
        assert_eq!(charge(0), 0);
        assert_eq!(charge(1), 1); // 0.15
        assert_eq!(charge(6), 1); // 0.9
        assert_eq!(charge(7), 2); // 1.05
        assert_eq!(charge(999), 150); // 149.85
        assert_eq!(charge(1_000), 150);
        assert_eq!(charge(1_001), 151); // 150.15
        assert_eq!(charge(89_315), 13_398); // 13,397.25
        assert_eq!(charge(100 * 1024), 15_360);
    }

    #[test]
    fn package_read_saturates() {
        assert_eq!(package_read_internal_gas(usize::MAX, u64::MAX), u64::MAX);
        assert_eq!(package_read_internal_gas(usize::MAX, 1_000), u64::MAX);
        assert_eq!(
            package_read_internal_gas(1, u64::MAX),
            u64::MAX.div_ceil(1000)
        );
        assert_eq!(
            u128::from(package_read_internal_gas(usize::MAX, 999)),
            (u128::from(u64::MAX) * 999).div_ceil(1000)
        );
    }

    fn config(version: u64) -> ProtocolConfig {
        ProtocolConfig::get_for_version(ProtocolVersion::new(version), Chain::Mainnet)
    }

    // Runs `charge` with a budget of `units` gas units and returns its result and the internal gas
    // left afterwards.
    fn run(
        config: &ProtocolConfig,
        units: u64,
        charge: impl FnOnce(&mut SuiGasStatus) -> Result<(), ExecutionError>,
    ) -> (bool, u64) {
        let mut status = SuiGasStatus::new(units * RGP, RGP, RGP, config).unwrap();
        let ok = charge(&mut status).is_ok();
        (ok, status.move_gas_status().remaining_internal_gas())
    }

    // Without a package rate (every protocol version before 139), a package read must deduct
    // exactly what `charge_storage_read` deducts, and splitting one charge into an object charge
    // and a package charge must leave the same result and gas as a single charge on the total.
    // Budgets just above, at, and below the total cover the out-of-gas edge.
    #[test]
    fn unset_package_rate_matches_storage_read() {
        // Protocol 137 uses gas_v2; 138 uses gas_v3.
        for version in [137, 138] {
            let config = config(version);
            assert_eq!(config.obj_access_cost_read_per_package_kb_as_option(), None);
            for &objects in SIZES {
                for &packages in SIZES {
                    let total = (objects + packages) as u64 * 15;
                    let exact = total.div_ceil(1000);
                    for units in [exact + 1, exact, exact.saturating_sub(1)] {
                        let single = run(&config, units, |s| s.charge_storage_read(packages));
                        let package =
                            run(&config, units, |s| s.charge_package_object_read(packages));
                        assert_eq!(
                            package, single,
                            "v{version} packages={packages} units={units}"
                        );

                        let combined = run(&config, units, |s| {
                            s.charge_storage_read(objects + packages)
                        });
                        let split = run(&config, units, |s| {
                            s.charge_storage_read(objects)?;
                            s.charge_package_object_read(packages)
                        });
                        assert_eq!(
                            split, combined,
                            "v{version} objects={objects} packages={packages} units={units}"
                        );
                    }
                }
            }
        }
    }

    // With a package rate, a package read deducts exactly `package_read_internal_gas`, in both gas
    // model implementations, and fails with no gas left when the budget is short by one unit.
    #[test]
    fn package_rate_charges_package_read_internal_gas() {
        let mut v137 = config(137);
        v137.set_obj_access_cost_read_per_package_kb_for_testing(150);
        for config in [v137, config(139)] {
            for &size in SIZES {
                let cost = package_read_internal_gas(size, 150);
                let units = cost.div_ceil(1000);
                let (ok, left) = run(&config, units, |s| s.charge_package_object_read(size));
                assert!(ok);
                assert_eq!(left, units * 1000 - cost, "size={size}");
                if units > 0 && cost > (units - 1) * 1000 {
                    let (ok, left) =
                        run(&config, units - 1, |s| s.charge_package_object_read(size));
                    assert!(!ok);
                    assert_eq!(left, 0);
                }
            }
        }
    }

    #[test]
    fn configured_rates() {
        assert_eq!(config(138).obj_access_cost_read_per_byte(), 15);
        assert_eq!(
            config(138).obj_access_cost_read_per_package_kb_as_option(),
            None
        );
        assert_eq!(config(139).obj_access_cost_read_per_byte(), 15);
        // 1% of 15 internal units per byte.
        assert_eq!(config(139).obj_access_cost_read_per_package_kb(), 150);
    }
}
