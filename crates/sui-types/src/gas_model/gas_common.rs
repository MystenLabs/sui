// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

mod checked {
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
            gas_balance += gas::get_gas_balance(gas_obj.as_object().ok_or(
                UserInputError::InvalidGasObject {
                    object_id: gas_obj.id(),
                },
            )?)? as u128;
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
        (((storage_rebate as u128 * storage_rebate_rate as u128)
        + (BASIS_POINTS / 2)) // integer rounding adds half of the BASIS_POINTS (denominator)
        / BASIS_POINTS) as u64
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
                return self
                    .unmetered_storage_rebate
                    .checked_add(storage_rebate)
                    .map(|total| {
                        self.unmetered_storage_rebate = total;
                        0
                    });
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
}
