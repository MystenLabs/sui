// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Benchmarks for constant pool loading overhead.
/// Measures the cost of loading constants of various types and sizes.
module 0x1::bench {
    const COUNT: u64 = 10_000;

    const C0: u64 = 100;
    const C1: u64 = 200;
    const C2: u64 = 300;
    const C3: u64 = 400;
    const C4: u64 = 500;
    const C5: u64 = 600;
    const C6: u64 = 700;
    const C7: u64 = 800;
    const C8: u64 = 900;
    const C9: u64 = 1000;
    const C10: u64 = 1100;
    const C11: u64 = 1200;
    const C12: u64 = 1300;
    const C13: u64 = 1400;
    const C14: u64 = 1500;
    const C15: u64 = 1600;

    const ADDR0: address = @0xCAFE;
    const ADDR1: address = @0xDEAD;
    const ADDR2: address = @0xBEEF;
    const ADDR3: address = @0xFACE;

    const BOOL_TRUE: bool = true;
    const BOOL_FALSE: bool = false;

    const U128_VAL: u128 = 340282366920938463463374607431768211455;
    const U256_VAL: u256 = 115792089237316195423570985008687907853269984665640564039457584007913129639935;

    const VEC_CONST: vector<u8> = b"hello world benchmark constant";

    public fun bench_load_u64_constants() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + C0 + C1 + C2 + C3;
            acc = acc + C4 + C5 + C6 + C7;
            acc = acc % 1000000;
            i = i + 1;
        }
    }

    public fun bench_load_many_u64_constants() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + C0 + C1 + C2 + C3 + C4 + C5 + C6 + C7
                      + C8 + C9 + C10 + C11 + C12 + C13 + C14 + C15;
            acc = acc % 1000000;
            i = i + 1;
        }
    }

    public fun bench_load_address_constants() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _a = ADDR0;
            let _b = ADDR1;
            let _c = ADDR2;
            let _d = ADDR3;
            i = i + 1;
        }
    }

    public fun bench_load_bool_constants() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            if (BOOL_TRUE) { acc = acc + 1 };
            if (!BOOL_FALSE) { acc = acc + 1 };
            i = i + 1;
        }
    }

    public fun bench_load_wide_constants() {
        let mut i: u64 = 0;
        let mut acc: u128 = 0;
        while (i < COUNT) {
            acc = (acc + U128_VAL) % 1000000;
            let _v = U256_VAL;
            i = i + 1;
        }
    }

    public fun bench_load_vector_constant() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _v = VEC_CONST;
            i = i + 1;
        }
    }
}
