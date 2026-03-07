// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Logical not on signed should error (not is for bool)
    fun not_i8() {
        let _x = !1i8;
    }

    fun not_i64() {
        let x: i64 = 5i64;
        let _y = !x;
    }
}
