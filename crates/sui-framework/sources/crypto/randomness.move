// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

/// WARNING: This module is currently insecure as the randomness source it depends on is not yet secure
/// (work-in-progress). Please use it for testing purposes only.

/// Randomness objects can only be created, set or consumed. They cannot be created and consumed
/// in the *same* transaction since it might allow validators decide whether to create and use those
/// objects *after* seeing the randomness they depend on.
///
/// - On creation, the object contains the epoch in which it was created and a unique object id.
///
/// - After the object creation transaction is committed, anyone can retrieve the BLS signature on
///   message "randomness":epoch:id from validators (signed using the Threshold-BLS key of that
///   epoch).
///
/// - Anyone that can mutate the object can set the randomness of the object by supplying the BLS
///   signature. This operation verifies the signature and sets the value of the randomness object
///   to be the hash of the signature.
///
///   Note that there is a single signature that could pass the verification for a specific object,
///   thus, the only options the owner of the object has after retrieving the signature (and learning
///   the randomness) is to either set the randomness or leave it unset. Applications that use
///   Randomness objects must make sure they handle both options (e.g., debit the user on object
///   creation so even if the user aborts, depending on the randomness it received, the application
///   is not harmed).
///
/// - Once set, the random value can be read/consumed.
///
/// This object can be used as a shared-/owned-object.
///
module sui::randomness {
    use std::hash::sha3_256;
    use std::option::{Self, Option};
    use std::vector;
    use sui::bcs;
    use sui::object::{Self, UID, ID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Set is called with an invalid signature.
    const EInvalidSignature: u64 = 0;
    /// Already set object cannot be set again.
    const EAlreadySet: u64 = 1;
    /// Supplied randomness is not of the right length.
    const EInvalidRndLength: u64 = 2;

    /// All signatures are prefixed with Domain.
    const Domain: vector<u8> = b"randomness";

    /// WARNING: This object is currently insecure as the randomness source it depends on is not yet secure
    /// (work-in-progress). Please use it for testing purposes only.
    struct Randomness<phantom T> has key {
        id: UID,
        epoch: u64,
        value: Option<vector<u8>>
    }

    public fun new<T: drop>(_w: T, ctx: &mut TxContext): Randomness<T> {
        Randomness<T> {
            id: object::new(ctx),
            epoch: tx_context::epoch(ctx),
            value: option::none(),
        }
        // TODO: Front load the fee.
    }

    public fun transfer<T>(self: Randomness<T>, to: address) {
        transfer::transfer(self, to);
    }

    public fun share_object<T>(self: Randomness<T>) {
        transfer::share_object(self);
    }

    /// Owner(s) can use this function for setting the randomness.
    public fun set<T>(self: &mut Randomness<T>, sig: vector<u8>) {
        assert!(option::is_none(&self.value), EAlreadySet);
        let msg = to_bytes(&Domain, self.epoch, &object::id(self));
        assert!(native_tbls_verify_signature(self.epoch, &msg, &sig), EInvalidSignature);
        let hashed = sha3_256(sig);
        self.value = option::some(hashed);
    }

    /// Delete the object.
    public fun destroy<T>(r: Randomness<T>) {
        let Randomness { id, epoch: _, value: _ } = r;
        object::delete(id);
    }

    /// Read the epoch of the object.
    public fun epoch<T>(self: &Randomness<T>): u64 {
        self.epoch
    }

    /// Read the current value of the object.
    public fun value<T>(self: &Randomness<T>): &Option<vector<u8>> {
        &self.value
    }

    fun to_bytes(domain: &vector<u8>, epoch: u64, id: &ID): vector<u8> {
        let buffer: vector<u8> = vector::empty();
        // All elements below are of fixed sizes.
        vector::append(&mut buffer, *domain);
        vector::append(&mut buffer, bcs::to_bytes(&epoch));
        vector::append(&mut buffer, object::id_to_bytes(id));
        buffer
    }

    /// Verify signature sig on message msg using the epoch's BLS key.
    native fun native_tbls_verify_signature(epoch: u64, msg: &vector<u8>, sig: &vector<u8>): bool;

    /// Helper functions to sign on messages in tests.
    native fun native_tbls_sign(epoch: u64, msg: &vector<u8>): vector<u8>;

    #[test_only]
    public fun sign<T>(self: &Randomness<T>): vector<u8> {
        let msg = to_bytes(&Domain, self.epoch, &object::id(self));
        native_tbls_sign(self.epoch, &msg)
    }

    // Helper functions for working with the outputs of Randomness

    // Converts the first 16 bytes of rnd to a u128 number and outputs its modulo with input n.
    // Since n is u64, the output is at most 2^{-64} biased assuming rnd is uniformly random.
    public fun safe_selection(n: u64, rnd: &vector<u8>): u64 {
        assert!(vector::length(rnd) >= 16, EInvalidRndLength);
        let m: u128 = 0;
        let i = 0;
        while (i < 16) {
            m = m << 8;
            let curr_byte = *vector::borrow(rnd, i);
            m = m + (curr_byte as u128);
            i = i + 1;
        };
        let n_128 = (n as u128);
        let module_128  = m % n_128;
        let res = (module_128 as u64);
        res
    }
}
