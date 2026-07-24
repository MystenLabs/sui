// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// '#[error]' constants may be used cross-module as plain values, and an '#[error]' constant
// may be defined by folding a cross-module constant and used in a local abort

module 0x42::a {

public(package) const PREFIX: vector<u8> = b"err: ";

#[error]
public(package) const ENotFound: vector<u8> = b"not found";

}

module 0x42::b {

use 0x42::a;

#[error]
const ELocal: vector<u8> = a::PREFIX;

public fun get(): vector<u8> { a::ENotFound }

public fun fail() { abort ELocal }

}
