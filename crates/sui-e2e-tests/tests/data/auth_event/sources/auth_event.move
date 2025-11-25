// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module auth_event::events;

use sui::event;

public struct E has copy, drop { value: u64 }

public struct LargeE has copy, drop {
    value: u64,
    data: vector<u8>,
}

public entry fun emit(value: u64) {
    event::emit_authenticated(E { value });
}

public entry fun emit_multiple(start_value: u64, count: u64) {
    let mut i = 0;
    while (i < count) {
        event::emit_authenticated(E { value: start_value + i });
        i = i + 1;
    };
}

public entry fun emit_large(value: u64, size: u64) {
    let mut data = vector::empty<u8>();
    let mut i = 0;
    while (i < size) {
        data.push_back(0);
        i = i + 1;
    };
    event::emit_authenticated(LargeE { value, data });
}
