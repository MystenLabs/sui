// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module auth_event::events;

use sui::event;

public struct E has copy, drop { value: u64 }

public entry fun emit(value: u64) {
    event::emit_authenticated(E { value });
}

public entry fun emit_multiple(values: vector<u64>) {
    let mut i = 0;
    while (i < values.length()) {
        event::emit_authenticated(E { value: values[i] });
        i = i + 1;
    };
}
