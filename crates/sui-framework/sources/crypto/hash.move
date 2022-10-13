// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::hash {
    /// @param key: HMAC key, arbitrary bytes
    /// @param msg: message to sign, arbitrary bytes
    /// Returns the 32 bytes output of HMAC-SHA2-256(key, msg).
    public native fun hmac_sha2_256(key: &vector<u8>, msg: &vector<u8>): vector<u8>;
}
