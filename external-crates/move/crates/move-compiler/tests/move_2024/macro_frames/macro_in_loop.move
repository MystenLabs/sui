// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests macro call inside a while-loop body.
module A::m {
    macro fun inc($x: u64): u64 {
        $x + 1
    }

    public fun test(): u64 {
        let mut i = 0;
        while (i < 10) {
            i = inc!(i);
        };
        i
    }
}
