// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x2::bench {
    const COUNT: u64 = 10_000;

    public struct Box<T> has copy, drop {
        value: T,
    }

    public struct Pair<T, U> has copy, drop {
        first: T,
        second: U,
    }

    public struct NestedBox<T> has copy, drop {
        inner: Box<T>,
        tag: u64,
    }

    public fun bench_generic_pack_u64() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _b = Box<u64> { value: i };
            i = i + 1;
        }
    }

    public fun bench_generic_pack_bool() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _b = Box<bool> { value: i % 2 == 0 };
            i = i + 1;
        }
    }

    public fun bench_generic_pack_pair() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _p = Pair<u64, bool> { first: i, second: true };
            i = i + 1;
        }
    }

    public fun bench_generic_call_u64() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + identity<u64>(i);
            i = i + 1;
        }
    }

    public fun bench_generic_call_bool() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let b = identity<bool>(i % 2 == 0);
            if (b) { acc = acc + 1 };
            i = i + 1;
        }
    }

    public fun bench_generic_call_struct() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let b = identity<Box<u64>>(Box { value: i });
            acc = acc + b.value;
            i = i + 1;
        }
    }

    public fun bench_generic_nested() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let nb = NestedBox<u64> { inner: Box { value: i }, tag: i + 1 };
            acc = acc + unwrap_nested<u64>(nb);
            i = i + 1;
        }
    }

    public fun bench_generic_multi_type_param() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let p = Pair<u64, u128> { first: i, second: (i as u128) };
            acc = acc + extract_first<u64, u128>(p);
            i = i + 1;
        }
    }

    public fun bench_generic_chain() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let b = wrap<u64>(i);
            let nb = wrap_nested<u64>(b, i + 1);
            acc = acc + unwrap_nested<u64>(nb);
            i = i + 1;
        }
    }

    fun identity<T>(x: T): T {
        x
    }

    fun wrap<T>(val: T): Box<T> {
        Box { value: val }
    }

    fun wrap_nested<T>(inner: Box<T>, tag: u64): NestedBox<T> {
        NestedBox { inner, tag }
    }

    fun unwrap_nested<T: copy + drop>(nb: NestedBox<T>): u64 {
        nb.tag
    }

    fun extract_first<T: copy + drop, U: drop>(p: Pair<T, U>): T {
        p.first
    }
}
