// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::vdf {

    /// Hash an arbitrary binary `message` to an input for the VDF.
    public fun hash_to_input(discriminant: &vector<u8>, message: &vector<u8>): vector<u8> {
        hash_to_input_internal(discriminant, message)
    }

    native  fun hash_to_input_internal(discriminant: &vector<u8>, message: &vector<u8>): vector<u8>;

    /// Verify the output and proof of a VDF with the given number of iterations.
    public fun vdf_verify(discriminant: &vector<u8>, input: &vector<u8>, output: &vector<u8>, proof: &vector<u8>, iterations: u64): bool {
        vdf_verify_internal(discriminant, input, output, proof, iterations)
    }

    native fun vdf_verify_internal(discriminant: &vector<u8>, input: &vector<u8>, output: &vector<u8>, proof: &vector<u8>, iterations: u64): bool;
}
