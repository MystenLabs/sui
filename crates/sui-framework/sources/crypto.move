// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::crypto {
    /// verify signature, if valid, recovered return public key, otherwise throw error.
    public native fun ecrecover(signature: vector<u8>, hashed_msg: vector<u8>): vector<u8>;
    
    /// Hash the input bytes using keccak256 and returns 32 bytes.
    public native fun keccak256(data: vector<u8>): vector<u8>;
}
