// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module which defines hash functions.
module sui::hash {
    /// @param data: Arbitrary binary data to hash
    /// Hash the input bytes using Blake2b-256 and returns 32 bytes.
    native public fun blake2b256(data: &vector<u8>): vector<u8>;

    /// @param data: Arbitrary binary data to hash
    /// Hash the input bytes using keccak256 and returns 32 bytes.
    native public fun keccak256(data: &vector<u8>): vector<u8>;
}
