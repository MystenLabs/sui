// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Benchmarks for abort propagation overhead.
/// Measures the cost of abort unwinding through various call depths.
module 0x2::bench {
    const COUNT: u64 = 10_000;

    public fun bench_abort_shallow() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            try_abort_depth_1(false);
            i = i + 1;
        }
    }

    public fun bench_abort_deep() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            try_abort_depth_8(false);
            i = i + 1;
        }
    }

    public fun bench_assert_pass() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            assert!(i < COUNT);
            assert!(true);
            i = i + 1;
        }
    }

    public fun bench_conditional_abort_never() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            acc = safe_div(i + 1, 1);
            acc = acc + safe_div(i + 1, 2);
            i = i + 1;
        }
    }

    public fun bench_nested_assert() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            validate_range(i, 0, COUNT);
            i = i + 1;
        }
    }

    fun try_abort_depth_1(should_abort: bool) {
        if (should_abort) abort 1;
    }

    fun try_abort_depth_8(should_abort: bool) {
        depth_7(should_abort);
    }

    fun depth_7(should_abort: bool) { depth_6(should_abort); }
    fun depth_6(should_abort: bool) { depth_5(should_abort); }
    fun depth_5(should_abort: bool) { depth_4(should_abort); }
    fun depth_4(should_abort: bool) { depth_3(should_abort); }
    fun depth_3(should_abort: bool) { depth_2(should_abort); }
    fun depth_2(should_abort: bool) { depth_1(should_abort); }
    fun depth_1(should_abort: bool) {
        if (should_abort) abort 1;
    }

    fun safe_div(a: u64, b: u64): u64 {
        assert!(b != 0);
        a / b
    }

    fun validate_range(val: u64, min: u64, max: u64) {
        assert!(val >= min);
        assert!(val < max);
    }
}
