// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::base64 {
    native public fun base64_encode(bytes: vector<u8>): vector<u8>;

    native public fun base64_decode(bytes: vector<u8>): vector<u8>;
}