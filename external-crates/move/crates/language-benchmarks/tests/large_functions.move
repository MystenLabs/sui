// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Benchmarks for large function bodies with many locals.
/// Measures frame allocation, local variable slot overhead, and
/// how the interpreter scales with function complexity.
module 0x2::bench {
    const COUNT: u64 = 10_000;

    public fun bench_many_locals_8() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let a0 = i;
            let a1 = a0 + 1;
            let a2 = a1 + 1;
            let a3 = a2 + 1;
            let a4 = a3 + 1;
            let a5 = a4 + 1;
            let a6 = a5 + 1;
            let _a7 = a6 + 1;
            i = i + 1;
        }
    }

    public fun bench_many_locals_16() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let a0 = i;      let a1 = a0 + 1;
            let a2 = a1 + 1; let a3 = a2 + 1;
            let a4 = a3 + 1; let a5 = a4 + 1;
            let a6 = a5 + 1; let a7 = a6 + 1;
            let a8 = a7 + 1; let a9 = a8 + 1;
            let a10 = a9 + 1; let a11 = a10 + 1;
            let a12 = a11 + 1; let a13 = a12 + 1;
            let a14 = a13 + 1; let _a15 = a14 + 1;
            i = i + 1;
        }
    }

    public fun bench_many_locals_32() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let a0 = i;       let a1 = a0 + 1;
            let a2 = a1 + 1;  let a3 = a2 + 1;
            let a4 = a3 + 1;  let a5 = a4 + 1;
            let a6 = a5 + 1;  let a7 = a6 + 1;
            let a8 = a7 + 1;  let a9 = a8 + 1;
            let a10 = a9 + 1;  let a11 = a10 + 1;
            let a12 = a11 + 1; let a13 = a12 + 1;
            let a14 = a13 + 1; let a15 = a14 + 1;
            let a16 = a15 + 1; let a17 = a16 + 1;
            let a18 = a17 + 1; let a19 = a18 + 1;
            let a20 = a19 + 1; let a21 = a20 + 1;
            let a22 = a21 + 1; let a23 = a22 + 1;
            let a24 = a23 + 1; let a25 = a24 + 1;
            let a26 = a25 + 1; let a27 = a26 + 1;
            let a28 = a27 + 1; let a29 = a28 + 1;
            let a30 = a29 + 1; let _a31 = a30 + 1;
            i = i + 1;
        }
    }

    public fun bench_call_wide_args() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + wide_args(i, i+1, i+2, i+3, i+4, i+5, i+6, i+7);
            i = i + 1;
        }
    }

    public fun bench_call_many_returns() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let (a, b, c, d) = multi_return(i);
            acc = acc + a + b + c + d;
            i = i + 1;
        }
    }

    public fun bench_deep_let_chain() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let v = {
                let a = i + 1;
                let b = a * 2;
                let c = b + 3;
                let d = c / 2;
                let e = d % 100;
                let f = e + a;
                let g = f * b;
                g % 10000
            };
            acc = acc + v;
            i = i + 1;
        }
    }

    fun wide_args(
        a0: u64, a1: u64, a2: u64, a3: u64,
        a4: u64, a5: u64, a6: u64, a7: u64,
    ): u64 {
        a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7
    }

    fun multi_return(x: u64): (u64, u64, u64, u64) {
        (x, x + 1, x + 2, x + 3)
    }
}
