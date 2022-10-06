// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// WARNING: Like all files in the examples section, this code is unaudited
/// and should NOT be running in production. Using the code unaudited could potentially
/// result in lost of funds from hacks, and leakage of transaction amounts.

/// An example implementation of a 'private balance' that stores a balance
/// that can be spent, but has a value that is known only to the owner of the balance.
module fungible_tokens::private_balance {
    /// For when trying to destroy a non-zero balance.
    const ENonZero: u64 = 0;
    /// For when an overflow is happening on Supply operations.
    const EOverflow: u64 = 1;
    /// For when trying to withdraw more than there is.
    const ENotEnough: u64 = 2;

    /// The maximum unsigned bits that the coin value should be
    const MAX_COIN_BIT: u64 = 64;

    use sui::bulletproofs::{verify_full_range_proof};
    use sui::elliptic_curve::{Self as ec, RistrettoPoint};
    use std::option::{Self, Option};

    /// A Supply of T. Used for minting and burning.
    /// Wrapped into a `TreasuryCap` in the `PrivateCoin` module.
    struct Supply<phantom T> has store {
        value: u64
    }

    /// Storable balance - an inner struct of a PrivateCoin type.
    /// Can be used to store coins which don't need to have the
    /// key ability.
    /// Helpful in representing a PrivateCoin without having to create a stand-alone object.
    struct PrivateBalance<phantom T> has store {
        commitment: RistrettoPoint, // Stores a Pedersen commitment to the value of the coin
        value: Option<u64> // In the case that someone wants to open their value - this number will be public
    }

    /// Get the pedersen commitment to the value of the coin.
    public fun commitment<T>(balance: &PrivateBalance<T>): RistrettoPoint {
        balance.commitment
    }

    /// Get the value stored by the coin. If the coin is private, will return Option::None.
    public fun value<T>(balance: &PrivateBalance<T>): Option<u64> {
        balance.value
    }

    /// Get the `Supply` value.
    public fun supply_value<T>(supply: &Supply<T>): u64 {
        supply.value
    }

    /// Create a new supply for type T.
    public fun create_supply<T: drop>(_witness: T): Supply<T> {
        Supply {
            value: 0
        }
    }

    /// Increase supply by `value` and create a new `PrivateBalance<T>` with this value.
    /// The new `PrivateBalance<T>` that is created by this function has a blinding_factor set
    /// to 0, and is public by default.
    ///
    /// The first minted private balances never hides its value, because it is
    /// important for public auditability that users know the max supply of the coin.
    public fun increase_supply<T>(self: &mut Supply<T>, value: u64): PrivateBalance<T> {
        assert!(value < (18446744073709551615u64 - self.value), EOverflow);
        self.value = self.value + value;
        let commitment = ec::create_pedersen_commitment(ec::new_scalar_from_u64(value), ec::new_scalar_from_u64(0));
        PrivateBalance {
            commitment,
            value: option::some(value)
        }
    }

    /// Create a zero `PrivateBalance` for currency type `T`.
    /// This is essentially a PedersenCommitment with value = 0, and blinding factor = 0.
    public fun zero<T>(): PrivateBalance<T> {
        // TODO: For optimization, pre-compute this and store somewhere.
        let commitment = ec::create_pedersen_commitment(ec::new_scalar_from_u64(0), ec::new_scalar_from_u64(0));
        PrivateBalance {
            commitment,
            value: option::some(0)
        }
    }

    /// Reveals the balance stored in `self`. The correct value and blinding factor of the coin
    /// must be provided to this function, otherwise the call will be aborted. After calling this function,
    /// anyone will be able to openly read the value of the balance. Note that after calling this function,
    /// the blinding factor of `self` will be set to 0.
    public fun open_balance<T>(self: &mut PrivateBalance<T>, value: u64, blinding_factor: vector<u8>) {
        let commitment = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(value),
            ec::new_scalar_from_bytes(blinding_factor)
        );
        assert!(ec::bytes(&commitment) == ec::bytes(&self.commitment), 0);
        // Bump blinding factor to down to zero
        let blinding_point = ec::create_pedersen_commitment(ec::new_scalar_from_u64(0), ec::new_scalar_from_bytes(blinding_factor));
        self.commitment = ec::subtract(&self.commitment, &blinding_point);
        // Open the value
        self.value = option::some(value)
    }

    /// Join two balances together. Note that the resulting coin is only public if both joined coins are also public.
    /// Overflows above u64 are not checked, as this is already checked during the minting process.
    public fun join<T>(self: &mut PrivateBalance<T>, other: PrivateBalance<T>) {
        let new_value: Option<u64> = option::none();
        // If both coins are already revealed,
        if (option::is_some(&self.value) && option::is_some(&other.value)) {
            new_value = option::some(*option::borrow(&self.value) + *option::borrow(&other.value));
        };
        let PrivateBalance { commitment, value: _ } = other;
        self.commitment = ec::add(&self.commitment, &commitment);
        self.value = new_value
    }

    /// Split a `PrivateBalance` and take a private sub-balance from it. `self` always becomes private after this function.
    public fun split<T>(self: &mut PrivateBalance<T>, new_commitment: RistrettoPoint, proof: vector<u8>): PrivateBalance<T> {
        // We start with coin A (self), we want to split it to B (new_commitment) and A_new = A - B.
        self.commitment = ec::subtract(&self.commitment, &new_commitment);
        self.value = option::none();
        // In order to prevent new coins being minted, Open(A_new) = Open(A) - Open(B) must hold.
        // It is clear to see that as long as |A| >= |B| holds, then the above equation also holds
        verify_full_range_proof(&proof, &self.commitment, MAX_COIN_BIT);
        PrivateBalance {
            commitment: new_commitment,
            value: option::none()
        }
    }

    /// Takes a public sub-balance from `self`. Note that `self` retains its privacy option after this function.
    public fun split_to_public<T>(self: &mut PrivateBalance<T>, value: u64, proof: vector<u8>): PrivateBalance<T> {
        // TODO OPTIMIZATION:
        // 1. Add functionality for Scalar to Point multiplication of the Pedersen Commitment base in FastCrypto.
        // 2. Add Default base Ristretto Points (value and blinding) for Pedersen Commitments on Sui somewhere.
        // 3. Replace this with just ec::scalar_to_point(value, PEDERSEN_BASE)
        let new_commitment = ec::create_pedersen_commitment(ec::new_scalar_from_u64(value), ec::new_scalar_from_u64(0));
        self.commitment = ec::subtract(&self.commitment, &new_commitment);
        if (option::is_some(&self.value)) {
            // If the value for both coins are public, we can forego the range proof
            self.value = option::some(*option::borrow(&self.value) - value);
            assert!(*option::borrow(&self.value) >= value, 0)
        } else {
            self.value = option::none();
            verify_full_range_proof(&proof, &self.commitment, MAX_COIN_BIT);
        };
        PrivateBalance {
            commitment: new_commitment,
            value: option::some(value)
        }
    }

    #[test_only]
    /// Create a `PrivacyBalance` of any coin for testing purposes.
    public fun create_for_testing<T>(value: u64): PrivateBalance<T> {
        let commitment = ec::create_pedersen_commitment(ec::new_scalar_from_u64(value), ec::new_scalar_from_u64(0));
        PrivateBalance {
            commitment,
            value: option::none()
        }
    }

    #[test_only]
    /// Destroy a `PrivacyBalance` with any value in it for testing purposes.
    public fun destroy_for_testing<T>(self: PrivateBalance<T>): RistrettoPoint {
        let PrivateBalance { commitment, value: _ } = self;
        commitment
    }

    #[test_only]
    /// Create a `Supply` of any coin for testing purposes.
    public fun create_supply_for_testing<T>(value: u64): Supply<T> {
        Supply { value }
    }
}
