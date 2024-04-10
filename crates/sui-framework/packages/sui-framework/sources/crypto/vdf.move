// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::vdf {

    /// Error code for invalid input
    const EInvalidInput: u64 = 0;

    /// The largest allowed byte length of the input to the VDF.
    const MAX_INPUT_LENGTH: u64 = 384;

    /// Hash an arbitrary binary `message` to a class group element to be used as input for `vdf_verify`.
    ///
    /// The `discriminant` defines what class group to use and should be the same as used in `vdf_verify`. The
    /// `discriminant` should be encoded as a big-endian encoding of the absolute value of the negative discriminant.
    public fun hash_to_input(discriminant: &vector<u8>, message: &vector<u8>): vector<u8> {
        // We allow up to 3072 bit discriminants
        assert!(discriminant.length() <= MAX_INPUT_LENGTH, EInvalidInput);
        hash_to_input_internal(discriminant, message)
    }

    native fun hash_to_input_internal(discriminant: &vector<u8>, message: &vector<u8>): vector<u8>;

    /// Verify the output and proof of a VDF with the given number of iterations. The `input`, `output` and `proof`
    /// are all class group elements represented by triples `(a,b,c)` such that `b^2 - 4ac = discriminant`. They should
    /// be encoded in the following format:
    ///
    /// `a_len` (2 bytes, big endian) | `a` as unsigned big endian bytes | `b_len` (2 bytes, big endian) | `b` as signed
    /// big endian bytes
    ///
    /// Note that `c` is omitted because it may be computed from `a` and `b` and `discriminant`.
    ///
    /// The `discriminant` defines what class group to use and should be the same as used in `hash_to_input`. The
    /// `discriminant` should be encoded as a big-endian encoding of the absolute value of the negative discriminant.
    /// 
    /// This uses Wesolowski's VDF construction over imaginary class groups as described in Wesolowski (2020), 
    /// 'Efficient Verifiable Delay Functions.', J. Cryptol. 33, and is compatible with the VDF implementation in 
    /// fastcrypto.
    public fun vdf_verify(discriminant: &vector<u8>, input: &vector<u8>, output: &vector<u8>, proof: &vector<u8>, iterations: u64): bool {
        // We allow up to 3072 bit discriminants
        assert!(discriminant.length() <= MAX_INPUT_LENGTH, EInvalidInput);
        vdf_verify_internal(discriminant, input, output, proof, iterations)
    }

    native fun vdf_verify_internal(discriminant: &vector<u8>, input: &vector<u8>, output: &vector<u8>, proof: &vector<u8>, iterations: u64): bool;
}
