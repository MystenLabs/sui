// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module basics::slow;

use sui::clock;

/// Entry points that can be slow to execute.

/// create `n` vectors of size `size` bytes.
public fun slow(mut n: u64, size: u64) {
    let mut top_level = vector[];
    while (n > 0) {
        let mut contents = vector[];
        let mut i = size;
        while (i > 0) {
            vector::push_back(&mut contents, 0u256);
            i = i - 1;
        };
        vector::push_back(&mut top_level, contents);
        n = n - 1;
    };
}    

/// bimodal alternates between slow and fast execution every 10 seconds
public fun bimodal(clock: &clock::Clock) {
    let t = clock.timestamp_ms();
    if ((t / 10000) % 2 == 0) {
        slow(100, 100);
    } else {
        slow(10, 10);
    }
}