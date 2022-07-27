// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::crypto {
    /// TO verify signature, if valid, return recovered public key, otherwise throw error.
    public native fun ecrecover(signature: vector<u8>, hashed_msg: vector<u8>): vector<u8>;
}
