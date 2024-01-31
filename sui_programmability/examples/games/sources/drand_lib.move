// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Helper module for working with drand outputs.
/// Currently works with chain 52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971 (quicknet).
///
/// See examples of how to use this in drand_based_lottery.move and drand_based_scratch_card.move.
///
/// If you want to use this module with the default network which has a 30s period, you need to change the public key,
/// genesis time and include the previous signature in verify_drand_signature. See https://drand.love/developer/ or the
/// previous version of this file: https://github.com/MystenLabs/sui/blob/92df778310679626f00bc4226d7f7a281322cfdd/sui_programmability/examples/games/sources/drand_lib.move
module games::drand_lib {
    use std::hash::sha2_256;
    use std::vector;

    use sui::bls12381;

    /// Error codes
    const EInvalidRndLength: u64 = 0;
    const EInvalidProof: u64 = 1;

    /// The genesis time of chain 52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971.
    const GENESIS: u64 = 1692803367;
    /// The public key of chain 52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971.
    const DRAND_PK: vector<u8> =
        x"83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a";

    /// The time in seconds between randomness beacon rounds.
    const PERIOD: u64 = 3;

    /// Check that a given epoch time has passed by verifying a drand signature from a later time.
    /// round must be at least (epoch_time - GENESIS)/PERIOD + 1).
    public fun verify_time_has_passed(epoch_time: u64, sig: vector<u8>, round: u64) {
        assert!(epoch_time <= GENESIS + PERIOD * (round - 1), EInvalidProof);
        verify_drand_signature(sig, round);
    }

    /// Check a drand output.
    public fun verify_drand_signature(sig: vector<u8>, round: u64) {
        // Convert round to a byte array in big-endian order.
        let round_bytes: vector<u8> = vector[0, 0, 0, 0, 0, 0, 0, 0];
        let i = 7;

        // Note that this loop never copies the last byte of round_bytes, though it is not expected to ever be non-zero.
        while (i > 0) {
            let curr_byte = round % 0x100;
            let curr_element = vector::borrow_mut(&mut round_bytes, i);
            *curr_element = (curr_byte as u8);
            round = round >> 8;
            i = i - 1;
        };

        // Compute sha256(prev_sig, round_bytes).
        let digest = sha2_256(round_bytes);
        // Verify the signature on the hash.
        let drand_pk = DRAND_PK;
        assert!(bls12381::bls12381_min_sig_verify(&sig, &drand_pk, &digest), EInvalidProof);
    }

    /// Derive a uniform vector from a drand signature.
    public fun derive_randomness(drand_sig: vector<u8>): vector<u8> {
        sha2_256(drand_sig)
    }

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
