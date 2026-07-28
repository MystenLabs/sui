// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Deprecation warnings fire on cross-module constant uses

module 0x42::a {

#[deprecated(note = b"use NEW instead")]
public(package) const OLD: u64 = 1;
public(package) const NEW: u64 = 2;

}

module 0x42::b {

use 0x42::a;

const D: u64 = a::OLD;

public fun old(): u64 { a::OLD }
public fun d(): u64 { D }
public fun new(): u64 { a::NEW }

}
