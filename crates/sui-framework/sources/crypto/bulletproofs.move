// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::bulletproofs {
    use sui::elliptic_curve::{Self as ec, RistrettoPoint};

    /// Only bit_length = 64, 32, 16, 8 will work.
    native fun native_verify_full_range_proof(proof: &vector<u8>, commitment: &vector<u8>, bit_length: u64);

    /// @param proof: The bulletproof
    /// @param commitment: The commitment which we are trying to verify the range proof for
    /// @param bit_length: The bit length that we prove the committed value is whithin. Note that bit_length must be either 64, 32, 16, or 8.
    ///
    /// If the range proof is valid, execution succeeds, else panics.
    public fun verify_full_range_proof(proof: &vector<u8>, commitment: &RistrettoPoint, bit_length: u64) {
        native_verify_full_range_proof(proof, &ec::bytes(commitment), bit_length)
    }
}
