// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::ecvrf {

    native fun native_ecvrf_verify(hash: &vector<u8>, alpha_string: &vector<u8>, public_key: &vector<u8>, proof: &vector<u8>): bool;

    public fun ecvrf_verify(hash: &vector<u8>, alpha_string: &vector<u8>, public_key: &vector<u8>, proof: &vector<u8>): bool {
        native_ecvrf_verify(hash, alpha_string, public_key, proof)
    }
}
