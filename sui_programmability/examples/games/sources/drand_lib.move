// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Helper module for working with drand outputs.
/// Currently works with chain 8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce.
///
/// See examples in drand_based_lottery.move.
///
module games::drand_lib {
    use std::hash::sha2_256;
    use std::vector;

    use sui::bls12381;

    /// Error codes
    const EInvalidRndLength: u64 = 0;
    const EInvalidProof: u64 = 1;

    /// The genesis time of chain 8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce.
    const GENESIS: u64 = 1595431050;
    /// The public key of chain 8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce.
    const DRAND_PK: vector<u8> =
        x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";

    /// Check that a given epoch time has passed by verifying a drand signature from a later time.
    /// round must be at least (epoch_time - GENESIS)/30 + 1).
    public fun verify_time_has_passed(epoch_time: u64, sig: vector<u8>, prev_sig: vector<u8>, round: u64) {
        assert!(epoch_time <= GENESIS + 30 * (round - 1), EInvalidProof);
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
}
