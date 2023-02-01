// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::ecvrf {

    /// @param hash: The hash/output from a ECVRF to be verified.
    /// @param alpha_string: Input/seed to the ECVRF used to generate the output.
    /// @param public_key: The public key corresponding to the private key used to generate the output.
    /// @param proof: The proof of validity of the output.
    /// A native move wrapper around the Ristretto ECVRF. Returns true if the proof is valid and corresponds to the given output.
    native fun native_ecvrf_verify(hash: &vector<u8>, alpha_string: &vector<u8>, public_key: &vector<u8>, proof: &vector<u8>): bool;

    /// @param hash: The hash/output from a ECVRF to be verified.
    /// @param alpha_string: Input/seed to the ECVRF used to generate the output.
    /// @param public_key: The public key corresponding to the private key used to generate the output.
    /// @param proof: The proof of validity of the output.
    /// Verify a proof for a Ristretto ECVRF. Returns true if the proof is valid and corresponds to the given output.
    public fun ecvrf_verify(hash: &vector<u8>, alpha_string: &vector<u8>, public_key: &vector<u8>, proof: &vector<u8>): bool {
        native_ecvrf_verify(hash, alpha_string, public_key, proof)
    }
}
