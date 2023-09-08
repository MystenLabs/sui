// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module which defines hash functions. Note that Sha-256 and Sha3-256 is available in the std::hash module in the
/// standard library.
module sui::hash {
    /// Error if the input to the Poseidon hash function is invalid: Either if
    /// more than 32 inputs are provided or if any of the inputs are larger than
    /// the BN254 field size.
    const EInvalidPoseidonInput: u64 = 0;

    /// @param data: Arbitrary binary data to hash
    /// Hash the input bytes using Blake2b-256 and returns 32 bytes.
    native public fun blake2b256(data: &vector<u8>): vector<u8>;

    /// @param data: Arbitrary binary data to hash
    /// Hash the input bytes using keccak256 and returns 32 bytes.
    native public fun keccak256(data: &vector<u8>): vector<u8>;

    /// @param data: Vector of BN254 field elements to hash.
    /// Hash the inputs using poseidon_bn254 and returns a BN254 field element.
    /// The number of inputs cannot exceed 32 and each element has to be a BN254
    /// field element in canonical representation so they cannot be larger than
    /// the BN254 field size, p = 0x2523648240000001BA344D80000000086121000000000013A700000000000013.
    native public fun poseidon_bn254(data: &vector<u256>): u256;
}
