// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module for working with drand (https://drand.love/), a distributed randomness beacon.
/// Hardcoded to work with the main drand chain 8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce
/// WARNING: Using this module places the drand committee (and all Drand committees of the past) on
/// a) the shared secret's secrecy (no collusion)
/// b) producing new rounds every PERIOD seconds (only relevant for using `drand::unsafe_unix_time_seconds` as a time reference
module sui::drand {
    use std::hash::sha2_256;
    use std::vector;

    use sui::bls12381;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;

    // Error codes
    /// Expected a 16 byte RND, but got a different length
    const EInvalidRndLength: u64 = 0;
    /// Could not verify signature on Drand
    const EInvalidProof: u64 = 1;
    /// Trying to advance to a round that has already passed
    const ERoundAlreadyPassed: u64 = 2;

    // Constants
    const CHAIN: vector<u8> = x"8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce";
    /// The genesis time of CHAIN
    const GENESIS: u64 = 1595431050;
    /// The public key of CHAIN
    const DRAND_PK: vector<u8> =
        x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
    /// Time in seconds between randomness beacon rounds for CHAIN
    const PERIOD: u64 = 30;

    /// Shared object recording drand progress. Anyone can advance the round by providing a valid signature
    struct Drand has key {
        id: UID,
        /// Most recent drand round we have seen
        previous_round: u64,
        /// Signature from the last drand round
        previous_sig: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        // initialize at a fairly recent round
        transfer::share_object(
            Drand {
                id: object::new(ctx),
                previous_round: 2475343,
                previous_sig: x"880c20aa83669a234f7b633917e43b7afb4e72421f0f520f7f570559356ed72e15867a77513346fbd37401709fc2192c1830936f23a751c228d091527290b304f2c6fa93851ca92c5ae7c3f7d7e0d06c65b561a6d6afeabce080f442f68d3845"
            }
        )
    }

    /// Advance `drand` by one round
    public fun advance(drand: &mut Drand, sig: vector<u8>) {
        let round = drand.previous_round + 1;
        verify_drand_signature(sig, drand.previous_sig, round);
        drand.previous_sig = sig;
        drand.previous_round = round;
    }

    /// Advance `drand` by an arbitrary number of rounds.
    /// Aborts if `round` is smaler than the most recent round `drand` has seen
    public fun jump(drand: &mut Drand, sig: vector<u8>, previous_sig: vector<u8>, round: u64) {
        assert!(round > drand.previous_round, ERoundAlreadyPassed);
        verify_drand_signature(sig, previous_sig, round);
        drand.previous_sig = sig;
        drand.previous_round = round;
    }

    /// Return the most recent round we have seen
    public fun round(drand: &Drand): u64 {
        drand.previous_round
    }

    /// Return the most recent signature we have seen
    public fun sig(drand: &Drand): vector<u8> {
        drand.previous_sig
    }

    /// Return the most recent Unix time we have seen in seconds, according to drand.
    /// This time is coarse-grained--it will only advance by increments of PERIOD (30 seconds).
    /// *WARNING*: Although drand is supposed to produce a new round every 30 seconds
    /// and always has since GENESIS, it is possible for the gaps between rounds to be > or < 30 seconds, and future.
    /// versions of the protocol may change PERIOD.
    /// Thus, this function should only be used as an approximation of Unix time for non-production use-cases
    /// --for anything security-critical, please use either Sui epoch time, and orcale, or the forthcoming
    /// protocol time feature https://github.com/MystenLabs/sui/issues/226.
    /// Finally, note that this is not an up-to-date time unless `drand` has been updated recently
    /// This function will likely be removed before Sui mainnet.
    public fun unsafe_unix_time_seconds(drand: &Drand): u64 {
        GENESIS + (PERIOD * drand.previous_round - 1)
    }

    /// Return `true` if `unix_timestamp` is confirmed to be in the past based on drand updates.
    /// Note that a return value of `false` does *not* necesarily mean that `unix_timestamp` is in
    /// in the future--`drand` might just be out-of-date.
    /// *WARNING*: Although drand is supposed to produce a new round every 30 seconds
    /// and always has since GENESIS, it is possible for the gaps between rounds to be > or < 30 seconds, and future.
    /// versions of the protocol may change PERIOD.
    /// Thus, this function should only be used as an approximation of Unix time for non-production use-cases
    /// --for anything security-critical, please use either Sui epoch time, and orcale, or the forthcoming
    /// protocol time feature https://github.com/MystenLabs/sui/issues/226.
    /// Finally, note that this is not an up-to-date time unless `drand` has been updated recently
    /// This function will likely be removed before Sui mainnet.
    public fun unsafe_timestamp_has_passed(drand: &Drand, unix_timestamp: u64): bool {
        unix_timestamp < unsafe_unix_time_seconds(drand)
    }

    // functions that leverage drand without requiring access to the shared object

    /// Check that a given epoch time has passed by verifying a drand signature from a later time.
    /// round must be at least (epoch_time - GENESIS) / PERIOD + 1).
    public fun verify_time_has_passed(epoch_time: u64, sig: vector<u8>, prev_sig: vector<u8>, round: u64) {
        assert!(epoch_time <= GENESIS + PERIOD * (round - 1), ERoundAlreadyPassed);
        verify_drand_signature(sig, prev_sig, round);
    }

    /// Check a drand output.
    public fun verify_drand_signature(sig: vector<u8>, prev_sig: vector<u8>, round: u64) {
        // Convert round to a byte array in big-endian order.
        let round_bytes: vector<u8> = vector[0, 0, 0, 0, 0, 0, 0, 0];
        let i = 7;
        while (i > 0) {
            let curr_byte = round % 0x100;
            let curr_element = vector::borrow_mut(&mut round_bytes, i);
            *curr_element = (curr_byte as u8);
            round = round >> 8;
            i = i - 1;
        };

        // Compute sha256(prev_sig, round_bytes).
        vector::append(&mut prev_sig, round_bytes);
        let digest = sha2_256(prev_sig);
        // Verify the signature on the hash.
        assert!(bls12381::bls12381_min_pk_verify(&sig, &DRAND_PK, &digest), EInvalidProof);
    }

    /// Derive a uniform vector from a drand signature.
    public fun derive_randomness(drand_sig: vector<u8>): vector<u8> {
        sha2_256(drand_sig)
    }

    /// Converts the first 16 bytes of rnd to a u128 number and outputs its modulo with input n.
    /// Since n is u64, the output is at most 2^{-64} biased assuming rnd is uniformly random.
    public fun safe_selection(n: u64, rnd: vector<u8>): u64 {
        assert!(vector::length(&rnd) >= 16, EInvalidRndLength);
        let m: u128 = 0;
        let i = 0;
        while (i < 16) {
            m = m << 8;
            let curr_byte = *vector::borrow(&rnd, i);
            m = m + (curr_byte as u128);
            i = i + 1;
        };
        let n_128 = (n as u128);
        let module_128  = m % n_128;
        let res = (module_128 as u64);
        res
    }

    /// Return the chain ID of the drand instance used by this module
    public fun chain(): vector<u8> {
        CHAIN
    }

    /// Return the public key of the drand instance used by this module
    public fun public_key(): vector<u8> {
        DRAND_PK
    }

    /// Return the period of the drand instance used by this module
    public fun period(): u64 {
        PERIOD
    }

    #[test_only]
    public fun new(previous_round: u64, previous_sig: vector<u8>, ctx: &mut TxContext): Drand {
        Drand { id: object::new(ctx), previous_round, previous_sig }
    }
}

#[test_only]
module sui::drand_tests {
    use sui::drand;

    #[test]
    fun test_verify_time_has_passed_success() {
        // Taken from the output of
        // curl https://drand.cloudflare.com/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/public/8
        drand::verify_time_has_passed(
            1595431050 + 30*7, // exactly the 8th round
            x"b3ed3c540ef5c5407ea6dbf7407ca5899feeb54f66f7e700ee063db71f979a869d28efa9e10b5e6d3d24a838e8b6386a15b411946c12815d81f2c445ae4ee1a7732509f0842f327c4d20d82a1209f12dbdd56fd715cc4ed887b53c321b318cd7",
            x"ada04f01558359fec41abeee43c5762c4017476a1e64ad643d3378a50ac1f7d07ad0abf0ba4bada53e6762582d661a980adf6290b5fb1683dedd821fe192868d70624907b2cef002e3ee197acd2395f1406fb660c91337d505860ab306a4432e",
            8
        );
        drand::verify_time_has_passed(
            1595431050 + 30*7 - 10, // the 8th round - 10 seconds
            x"b3ed3c540ef5c5407ea6dbf7407ca5899feeb54f66f7e700ee063db71f979a869d28efa9e10b5e6d3d24a838e8b6386a15b411946c12815d81f2c445ae4ee1a7732509f0842f327c4d20d82a1209f12dbdd56fd715cc4ed887b53c321b318cd7",
            x"ada04f01558359fec41abeee43c5762c4017476a1e64ad643d3378a50ac1f7d07ad0abf0ba4bada53e6762582d661a980adf6290b5fb1683dedd821fe192868d70624907b2cef002e3ee197acd2395f1406fb660c91337d505860ab306a4432e",
            8
        );
    }

    #[test]
    #[expected_failure(abort_code = sui::drand::ERoundAlreadyPassed)]
    fun test_verify_time_has_passed_failure() {
        // Taken from the output of
        // curl https://drand.cloudflare.com/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/public/8
        drand::verify_time_has_passed(
            1595431050 + 30*8, // exactly the 9th round - 10 seconds
            x"b3ed3c540ef5c5407ea6dbf7407ca5899feeb54f66f7e700ee063db71f979a869d28efa9e10b5e6d3d24a838e8b6386a15b411946c12815d81f2c445ae4ee1a7732509f0842f327c4d20d82a1209f12dbdd56fd715cc4ed887b53c321b318cd7",
            x"ada04f01558359fec41abeee43c5762c4017476a1e64ad643d3378a50ac1f7d07ad0abf0ba4bada53e6762582d661a980adf6290b5fb1683dedd821fe192868d70624907b2cef002e3ee197acd2395f1406fb660c91337d505860ab306a4432e",
            8
        );
    }
}
