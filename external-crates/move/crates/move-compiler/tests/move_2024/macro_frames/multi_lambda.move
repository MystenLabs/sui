// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests macro with multiple lambda parameters — distinct Lambda frames.
module A::m {
    macro fun apply2($f: |u64| -> u64, $g: |u64| -> u64): u64 {
        $f(1) + $g(2)
    }

    public fun test(): u64 {
        apply2!(|x| x + 1, |y| y * 2)
    }
}
