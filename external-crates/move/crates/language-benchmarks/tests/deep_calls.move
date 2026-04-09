// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Benchmarks for deep call stack overhead.
/// Measures frame push/pop cost at increasing call depths.
module 0x2::bench {
    const COUNT: u64 = 10_000;

    public fun bench_call_depth_4() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + d4_a(i);
            i = i + 1;
        }
    }

    public fun bench_call_depth_8() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + d8_a(i);
            i = i + 1;
        }
    }

    public fun bench_call_depth_16() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + d16_a(i);
            i = i + 1;
        }
    }

    public fun bench_call_depth_32() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + d32_a(i);
            i = i + 1;
        }
    }

    public fun bench_recursive_depth_16() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + recurse(i, 16);
            i = i + 1;
        }
    }

    public fun bench_recursive_depth_32() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = acc + recurse(i, 32);
            i = i + 1;
        }
    }

    fun recurse(val: u64, depth: u64): u64 {
        if (depth == 0) {
            val
        } else {
            recurse(val + 1, depth - 1)
        }
    }

    // Depth-4 chain
    fun d4_a(x: u64): u64 { d4_b(x + 1) }
    fun d4_b(x: u64): u64 { d4_c(x + 1) }
    fun d4_c(x: u64): u64 { d4_d(x + 1) }
    fun d4_d(x: u64): u64 { x + 1 }

    // Depth-8 chain
    fun d8_a(x: u64): u64 { d8_b(x + 1) }
    fun d8_b(x: u64): u64 { d8_c(x + 1) }
    fun d8_c(x: u64): u64 { d8_d(x + 1) }
    fun d8_d(x: u64): u64 { d8_e(x + 1) }
    fun d8_e(x: u64): u64 { d8_f(x + 1) }
    fun d8_f(x: u64): u64 { d8_g(x + 1) }
    fun d8_g(x: u64): u64 { d8_h(x + 1) }
    fun d8_h(x: u64): u64 { x + 1 }

    // Depth-16 chain
    fun d16_a(x: u64): u64 { d16_b(x + 1) }
    fun d16_b(x: u64): u64 { d16_c(x + 1) }
    fun d16_c(x: u64): u64 { d16_d(x + 1) }
    fun d16_d(x: u64): u64 { d16_e(x + 1) }
    fun d16_e(x: u64): u64 { d16_f(x + 1) }
    fun d16_f(x: u64): u64 { d16_g(x + 1) }
    fun d16_g(x: u64): u64 { d16_h(x + 1) }
    fun d16_h(x: u64): u64 { d16_i(x + 1) }
    fun d16_i(x: u64): u64 { d16_j(x + 1) }
    fun d16_j(x: u64): u64 { d16_k(x + 1) }
    fun d16_k(x: u64): u64 { d16_l(x + 1) }
    fun d16_l(x: u64): u64 { d16_m(x + 1) }
    fun d16_m(x: u64): u64 { d16_n(x + 1) }
    fun d16_n(x: u64): u64 { d16_o(x + 1) }
    fun d16_o(x: u64): u64 { d16_p(x + 1) }
    fun d16_p(x: u64): u64 { x + 1 }

    // Depth-32 chain
    fun d32_a(x: u64): u64 { d32_b(x + 1) }
    fun d32_b(x: u64): u64 { d32_c(x + 1) }
    fun d32_c(x: u64): u64 { d32_d(x + 1) }
    fun d32_d(x: u64): u64 { d32_e(x + 1) }
    fun d32_e(x: u64): u64 { d32_f(x + 1) }
    fun d32_f(x: u64): u64 { d32_g(x + 1) }
    fun d32_g(x: u64): u64 { d32_h(x + 1) }
    fun d32_h(x: u64): u64 { d32_i(x + 1) }
    fun d32_i(x: u64): u64 { d32_j(x + 1) }
    fun d32_j(x: u64): u64 { d32_k(x + 1) }
    fun d32_k(x: u64): u64 { d32_l(x + 1) }
    fun d32_l(x: u64): u64 { d32_m(x + 1) }
    fun d32_m(x: u64): u64 { d32_n(x + 1) }
    fun d32_n(x: u64): u64 { d32_o(x + 1) }
    fun d32_o(x: u64): u64 { d32_p(x + 1) }
    fun d32_p(x: u64): u64 { d32_q(x + 1) }
    fun d32_q(x: u64): u64 { d32_r(x + 1) }
    fun d32_r(x: u64): u64 { d32_s(x + 1) }
    fun d32_s(x: u64): u64 { d32_t(x + 1) }
    fun d32_t(x: u64): u64 { d32_u(x + 1) }
    fun d32_u(x: u64): u64 { d32_v(x + 1) }
    fun d32_v(x: u64): u64 { d32_w(x + 1) }
    fun d32_w(x: u64): u64 { d32_x(x + 1) }
    fun d32_x(x: u64): u64 { d32_y(x + 1) }
    fun d32_y(x: u64): u64 { d32_z(x + 1) }
    fun d32_z(x: u64): u64 { d32_aa(x + 1) }
    fun d32_aa(x: u64): u64 { d32_bb(x + 1) }
    fun d32_bb(x: u64): u64 { d32_cc(x + 1) }
    fun d32_cc(x: u64): u64 { d32_dd(x + 1) }
    fun d32_dd(x: u64): u64 { d32_ee(x + 1) }
    fun d32_ee(x: u64): u64 { d32_ff(x + 1) }
    fun d32_ff(x: u64): u64 { x + 1 }
}
