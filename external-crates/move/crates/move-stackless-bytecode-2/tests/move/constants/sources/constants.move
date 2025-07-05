// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module constants::constants {
    /// The maximum value of a counter.
    const MAX_VALUE: u64 = 100;

    /// The default increment value for the counter.
    const DEFAULT_INCREMENT: u64 = 1;

    /// The default initial value for the counter.
    const DEFAULT_INITIAL_VALUE: u64 = 0;

    public fun compute(x: u64, y: u64): u64 {
        // Example function that uses constants
        let mut res = 0;
        let mut i = 0;
        if (x < MAX_VALUE && y < 10) {
            while (i < y) {
                res = res + x + DEFAULT_INCREMENT;
                i = i + 1;
            }
        } else {
            res = MAX_VALUE;
        };
        res * x + y + DEFAULT_INITIAL_VALUE
    }
}