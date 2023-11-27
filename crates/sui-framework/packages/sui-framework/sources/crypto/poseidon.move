// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module which defines instances of the poseidon hash functions.
module sui::poseidon {

    #[allow(unused_const)]
    /// Error if any of the inputs are larger than or equal to the BN254 field size.
    const ENonCanonicalInput: u64 = 0;

    /// @param data: Vector of BN254 field elements to hash.
    ///
    /// Hash the inputs using poseidon_bn254 and returns a BN254 field element.
    ///
    /// Each element has to be a BN254 field element in canonical representation so it must be smaller than the BN254
    /// scalar field size which is 21888242871839275222246405745257275088548364400416034343698204186575808495617.
    native public fun poseidon_bn254(data: &vector<u256>): u256;
}