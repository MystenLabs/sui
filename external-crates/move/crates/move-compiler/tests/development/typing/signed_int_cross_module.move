// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::helper {
    public fun get_i64(): i64 { 42i64 }
    public fun get_i8(): i8 { 1i8 }
    public fun take_i32(x: i32): i32 { x }
    public fun negate(x: i64): i64 { -x }

    public struct Wrapper has copy, drop {
        val: i64,
    }

    public fun wrap(v: i64): Wrapper { Wrapper { val: v } }
    public fun unwrap(w: Wrapper): i64 { w.val }
}

module 0x42::main {
    use 0x42::helper;

    fun use_cross_module() {
        let _a = helper::get_i64();
        let _b = helper::get_i8();
        let _c = helper::take_i32(10i32);
        let _d = helper::negate(5i64);
    }

    fun use_wrapper() {
        let w = helper::wrap(100i64);
        let _v = helper::unwrap(w);
    }
}
