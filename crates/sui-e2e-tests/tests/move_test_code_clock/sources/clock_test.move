// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code_clock::clock_test;

use sui::clock::{Self, Clock};
use sui::event;

const EAbortAfterRead: u64 = 7;

public struct NowMs has copy, drop {
    timestamp_ms: u64,
}

public entry fun emit_now_ms() {
    event::emit(NowMs { timestamp_ms: clock::now_ms() });
}

public entry fun assert_matches_clock(clock: &Clock) {
    assert!(clock::now_ms() == clock.timestamp_ms(), 0);
}

public entry fun read_then_abort() {
    let _ = clock::now_ms();
    abort EAbortAfterRead
}
