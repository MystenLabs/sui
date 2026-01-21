// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module which defines instances of the poseidon hash functions.
module sui::poseidon;

use sui::bcs;

/// Error if any of the inputs are larger than or equal to the BN254 field size.
const ENonCanonicalInput: u64 = 0;

/// Error if an empty vector is passed as input.
const EEmptyInput: u64 = 1;

/// Error if more than MAX_INPUTS inputs are given.
const ETooManyInputs: u64 = 2;

/// The field size for BN254 curve.
const BN254_MAX: u256 =
    21888242871839275222246405745257275088548364400416034343698204186575808495617u256;

/// The maximum number of inputs for the poseidon_bn254 function.
const MAX_INPUTS: u64 = 16;

/// @param data: Vector of BN254 field elements to hash.
///
/// Hash the inputs using poseidon_bn254 and returns a BN254 field element.
///
/// Each element has to be a BN254 field element in canonical representation so it must be smaller than the BN254
/// scalar field size which is 21888242871839275222246405745257275088548364400416034343698204186575808495617.
///
/// This function supports between 1 and 16 inputs. If you need to hash more than 16 inputs, some implementations
/// instead returns the root of a k-ary Merkle tree with the inputs as leafs, but since this is not standardized,
/// we leave that to the caller to implement if needed.
///
/// If the input is empty, the function will abort with EEmptyInput.
/// If more than 16 inputs are provided, the function will abort with ETooManyInputs.
public fun poseidon_bn254(data: &vector<u256>): u256 {
    assert!(data.length() > 0, EEmptyInput);
    assert!(data.length() <= MAX_INPUTS, ETooManyInputs);
    let b = data.map_ref!(|e| {
        assert!(*e < BN254_MAX, ENonCanonicalInput);
        bcs::to_bytes(e)
    });
    let binary_output = poseidon_bn254_internal(&b);
    bcs::new(binary_output).peel_u256()
}

/// @param data: Vector of BN254 field elements in little-endian representation.
///
/// Hash the inputs using poseidon_bn254 and returns a BN254 field element in little-endian representation.
native fun poseidon_bn254_internal(data: &vector<vector<u8>>): vector<u8>;
