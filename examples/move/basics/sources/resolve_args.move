// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Functions and types to test argument resolution in JSON-RPC.
module basics::resolve_args;

public struct Foo has key {
    id: UID,
}

public fun foo(
    _foo: &mut Foo,
    _bar: vector<Foo>,
    _name: vector<u8>,
    _index: u64,
    _flag: u8,
    _recipient: address,
    _ctx: &mut TxContext,
) {
    abort 0
}
