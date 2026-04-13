// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x2::bench {
    const COUNT: u64 = 10_000;

    public struct Pair has copy, drop {
        x: u64,
        y: u64,
    }

    public struct Wrapper has copy, drop {
        inner: Pair,
        tag: u64,
    }

    public fun bench_imm_borrow_local() {
        let mut i: u64 = 0;
        let val: u64 = 42;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let r = &val;
            acc = acc + *r;
            i = i + 1;
        }
    }

    public fun bench_mut_borrow_local() {
        let mut i: u64 = 0;
        let mut val: u64 = 0;
        while (i < COUNT) {
            let r = &mut val;
            *r = i;
            i = i + 1;
        }
    }

    public fun bench_imm_borrow_field() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        let p = Pair { x: 10, y: 20 };
        while (i < COUNT) {
            let rx = &p.x;
            let ry = &p.y;
            acc = acc + *rx + *ry;
            i = i + 1;
        }
    }

    public fun bench_mut_borrow_field() {
        let mut i: u64 = 0;
        let mut p = Pair { x: 0, y: 0 };
        while (i < COUNT) {
            let rx = &mut p.x;
            *rx = i;
            let ry = &mut p.y;
            *ry = i + 1;
            i = i + 1;
        }
    }

    public fun bench_nested_borrow_field() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        let w = Wrapper { inner: Pair { x: 5, y: 10 }, tag: 99 };
        while (i < COUNT) {
            let rx = &w.inner.x;
            let ry = &w.inner.y;
            let rt = &w.tag;
            acc = acc + *rx + *ry + *rt;
            i = i + 1;
        }
    }

    public fun bench_mut_nested_borrow_field() {
        let mut i: u64 = 0;
        let mut w = Wrapper { inner: Pair { x: 0, y: 0 }, tag: 0 };
        while (i < COUNT) {
            let rx = &mut w.inner.x;
            *rx = i;
            let ry = &mut w.inner.y;
            *ry = i + 1;
            let rt = &mut w.tag;
            *rt = i + 2;
            i = i + 1;
        }
    }

    public fun bench_freeze_ref() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        let mut p = Pair { x: 0, y: 0 };
        while (i < COUNT) {
            p.x = i;
            p.y = i + 1;
            let frozen = &p;
            acc = acc + frozen.x + frozen.y;
            i = i + 1;
        }
    }

    public fun bench_ref_pass_imm() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        let p = Pair { x: 7, y: 13 };
        while (i < COUNT) {
            acc = acc + read_pair(&p);
            i = i + 1;
        }
    }

    public fun bench_ref_pass_mut() {
        let mut i: u64 = 0;
        let mut p = Pair { x: 0, y: 0 };
        while (i < COUNT) {
            write_pair(&mut p, i);
            i = i + 1;
        }
    }

    fun read_pair(p: &Pair): u64 {
        p.x + p.y
    }

    fun write_pair(p: &mut Pair, val: u64) {
        p.x = val;
        p.y = val + 1;
    }
}
