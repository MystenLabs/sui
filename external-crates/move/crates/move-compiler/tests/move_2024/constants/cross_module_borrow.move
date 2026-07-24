// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Borrows of cross-module constants compile, and the implicit-copy warning still fires

module 0x42::a {

public(package) const MAX: u64 = 100;
public(package) const BYTES: vector<u8> = b"hi";

}

module 0x42::b {

use 0x42::a;

public fun deref(): u64 { *&a::MAX }

public fun borrow(): vector<u8> { *&a::BYTES }

}
