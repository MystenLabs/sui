// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example implementation of a 'private balance' that stores a balance
/// that can be spent, but has a value that is known only to the owner of the balance.
module fungible_tokens::private_balance {
    /// For when trying to destroy a non-zero balance.
    const ENonZero: u64 = 0;

    /// For when an overflow is happening on Supply operations.
    const EOverflow: u64 = 1;

    /// For when trying to withdraw more than there is.
    const ENotEnough: u64 = 2;

    use sui::crypto::{RistrettoPoint, create_pedersen_commitment, big_scalar_from_u64, big_scalar_to_vec, add_ristretto_point, subtract_ristretto_point};

    /// A Supply of T. Used for minting and burning.
    /// Wrapped into a `TreasuryCap` in the `Coin` module.
    struct Supply<phantom T> has store {
        value: u64
    }

    /// Storable balance - an inner struct of a Coin type.
    /// Can be used to store coins which don't need to have the
    /// key ability.
    /// Helpful in representing a Coin without having to create a stand-alone object.
    struct PrivateBalance<phantom T> has store {
        value: RistrettoPoint // Stores a Pedersen commitment
    }

    /// Get the `Supply` value.
    public fun supply_value<T>(supply: &Supply<T>): u64 {
        supply.value
    }

    /// Create a new supply for type T.
    public fun create_supply<T: drop>(_witness: T): Supply<T> {
        Supply { value: 0 }
    }

    /// Increase supply by `value` and create a new `Balance<T>` with this value. Requires
    /// that the user supplies a 'blinding factor' that is used to generate the pedersen commitment
    /// which holds the balance of the coin. Note that the first minted coin never hides its value,
    /// as it is important for public auditability to know the max supply of the coin.
    public fun increase_supply<T>(self: &mut Supply<T>, value: u64, blinding_factor: vector<u8>): PrivateBalance<T> {
        assert!(value < (18446744073709551615u64 - self.value), EOverflow);
        self.value = self.value + value;
        let commitment = create_pedersen_commitment(big_scalar_to_vec(big_scalar_from_u64(value)), blinding_factor);
        PrivateBalance { 
            value: commitment
        }
    }

    /// Create a zero `Balance` for type `T`.
    /// This is essentially a pedersen commitment with value = 0, and blinding factor = 0.
    public fun zero<T>(): PrivateBalance<T> {
        let commitment = create_pedersen_commitment(big_scalar_to_vec(big_scalar_from_u64(0)), big_scalar_to_vec(big_scalar_from_u64(0)));
        PrivateBalance { value: commitment }
    }

    /// Join two balances together.
    public fun join<T>(self: &mut PrivateBalance<T>, balance: PrivateBalance<T>) {
        let PrivateBalance { value } = balance;
        self.value = add_ristretto_point(&self.value, &value)
    }

    /// Split a `Balance` and take a sub balance from it.
    public fun split<T>(self: &mut PrivateBalance<T>, new_commitment: RistrettoPoint, proof: vector<u8>): PrivateBalance<T> {
        self.value = subtract_ristretto_point(&self.value, &new_commitment);
        PrivateBalance { value: new_commitment }
    }

    #[test_only]
    /// Create a `Balance` of any coin for testing purposes.
    public fun create_for_testing<T>(value: u64): PrivateBalance<T> {
        let commitment = create_pedersen_commitment(big_scalar_to_vec(big_scalar_from_u64(value)), big_scalar_to_vec(big_scalar_from_u64(0)));
        PrivateBalance { value: commitment }
    }

    #[test_only]
    /// Destroy a `Balance` with any value in it for testing purposes.
    public fun destroy_for_testing<T>(self: PrivateBalance<T>): RistrettoPoint {
        let PrivateBalance { value } = self;
        value
    }

    #[test_only]
    /// Create a `Supply` of any coin for testing purposes.
    public fun create_supply_for_testing<T>(value: u64): Supply<T> {
        Supply { value }
    }
}
