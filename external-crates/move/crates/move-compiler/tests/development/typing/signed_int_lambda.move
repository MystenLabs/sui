// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Higher-order function with signed int
    macro fun apply($f: |i64| -> i64, $x: i64): i64 {
        $f($x)
    }

    fun use_apply() {
        let _x = apply!(|x| -x, 5i64);
    }

    macro fun apply2($f: |i32, i32| -> i32, $a: i32, $b: i32): i32 {
        $f($a, $b)
    }

    fun use_apply2() {
        let _x = apply2!(|a, b| a + b, 3i32, 4i32);
    }
}
