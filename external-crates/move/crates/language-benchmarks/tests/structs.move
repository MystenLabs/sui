// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x2::bench {
    const COUNT: u64 = 10_000;

    public struct Simple has copy, drop {
        x: u64,
        y: u64,
    }

    public struct Wide has copy, drop {
        a: u64, b: u64, c: u64, d: u64,
        e: u64, f: u64, g: u64, h: u64,
    }

    public struct Nested has copy, drop {
        inner: Simple,
        value: u64,
    }

    public struct Deep has copy, drop {
        inner: Nested,
        flag: bool,
    }

    public fun bench_pack_simple() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _s = Simple { x: i, y: i + 1 };
            i = i + 1;
        }
    }

    public fun bench_unpack_simple() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let s = Simple { x: i, y: i + 1 };
            let Simple { x, y } = s;
            acc = acc + x + y;
            i = i + 1;
        }
    }

    public fun bench_pack_wide() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _w = Wide { a: i, b: i, c: i, d: i, e: i, f: i, g: i, h: i };
            i = i + 1;
        }
    }

    public fun bench_unpack_wide() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let w = Wide { a: i, b: i, c: i, d: i, e: i, f: i, g: i, h: i };
            let Wide { a, b, c, d, e, f, g, h } = w;
            acc = acc + a + b + c + d + e + f + g + h;
            i = i + 1;
        }
    }

    public fun bench_field_access() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        let w = Wide { a: 1, b: 2, c: 3, d: 4, e: 5, f: 6, g: 7, h: 8 };
        while (i < COUNT) {
            acc = acc + w.a + w.b + w.c + w.d + w.e + w.f + w.g + w.h;
            i = i + 1;
        }
    }

    public fun bench_pack_nested() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _n = Nested { inner: Simple { x: i, y: i + 1 }, value: i };
            i = i + 1;
        }
    }

    public fun bench_unpack_nested() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let n = Nested { inner: Simple { x: i, y: i + 1 }, value: i };
            let Nested { inner, value } = n;
            let Simple { x, y } = inner;
            acc = acc + x + y + value;
            i = i + 1;
        }
    }

    public fun bench_deep_nesting() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let d = Deep {
                inner: Nested { inner: Simple { x: i, y: i + 1 }, value: i },
                flag: true,
            };
            acc = acc + d.inner.inner.x + d.inner.inner.y + d.inner.value;
            i = i + 1;
        }
    }

    public fun bench_mut_field() {
        let mut i: u64 = 0;
        let mut s = Simple { x: 0, y: 0 };
        while (i < COUNT) {
            s.x = i;
            s.y = i + 1;
            i = i + 1;
        }
    }
}
