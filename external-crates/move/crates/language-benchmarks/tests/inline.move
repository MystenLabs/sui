// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Benchmarks for direct call inlining optimization.
///
/// These benchmarks measure the performance impact of inlining small functions
/// at the JIT translation level. Functions with 0-2 parameters are candidates
/// for inlining, which eliminates call overhead by embedding the callee's
/// bytecode directly into the caller.
///
/// Run with: cargo bench -p language-benchmarks -- inline
///
/// Compare with/without optimization by modifying VMConfig::optimize_bytecode
module 0x1::bench {
    const ITERATIONS: u64 = 100_000;

    /// Benchmark: Inlineable function with 0 parameters
    /// Tests the benefit of inlining simple constant-returning functions.
    public fun bench_inline_0_params() {
        let mut i: u64 = 0;
        let mut sum: u64 = 0;
        while (i < ITERATIONS) {
            sum = sum + get_constant();
            i = i + 1;
        }
    }

    /// Benchmark: Inlineable function with 1 parameter
    /// Tests the benefit of inlining single-parameter functions.
    public fun bench_inline_1_param() {
        let mut i: u64 = 0;
        let mut sum: u64 = 0;
        while (i < ITERATIONS) {
            sum = sum + double(i);
            i = i + 1;
        }
    }

    /// Benchmark: Inlineable function with 2 parameters
    /// Tests the benefit of inlining two-parameter functions.
    public fun bench_inline_2_params() {
        let mut i: u64 = 0;
        let mut sum: u64 = 0;
        while (i < ITERATIONS) {
            sum = add(sum, i);
            i = i + 1;
        }
    }

    /// Benchmark: Non-inlineable function with 3 parameters
    /// Control benchmark - this function should NOT be inlined (too many params).
    public fun bench_no_inline_3_params() {
        let mut i: u64 = 0;
        let mut sum: u64 = 0;
        while (i < ITERATIONS) {
            sum = add3(sum, i, 1);
            i = i + 1;
        }
    }

    /// Benchmark: Multiple inlineable calls per iteration
    /// Tests accumulated benefit of multiple inlined calls.
    public fun bench_inline_multiple_calls() {
        let mut i: u64 = 0;
        let mut sum: u64 = 0;
        while (i < ITERATIONS) {
            let a = get_constant();
            let b = double(i);
            let c = add(a, b);
            sum = add(sum, c);
            i = i + 1;
        }
    }

    /// Benchmark: Nested inlineable calls
    /// Tests inlining when one inlined function calls another.
    public fun bench_inline_nested() {
        let mut i: u64 = 0;
        let mut sum: u64 = 0;
        while (i < ITERATIONS) {
            sum = add(sum, quadruple(i));
            i = i + 1;
        }
    }

    /// Benchmark: Bool parameter inlining
    /// Tests inlining with non-integral parameter types.
    public fun bench_inline_bool_param() {
        let mut i: u64 = 0;
        let mut count: u64 = 0;
        while (i < ITERATIONS) {
            let flag = i % 2 == 0;
            if (negate(flag)) {
                count = count + 1;
            };
            i = i + 1;
        }
    }

    /// Benchmark: Mixed type parameters
    /// Tests inlining with heterogeneous parameter types.
    public fun bench_inline_mixed_params() {
        let mut i: u64 = 0;
        let mut count: u64 = 0;
        while (i < ITERATIONS) {
            if (check_threshold(@0x1, i)) {
                count = count + 1;
            };
            i = i + 1;
        }
    }

    // =========================================================================
    // Helper functions (inlining candidates)
    // =========================================================================

    /// 0-param function - always inlined
    fun get_constant(): u64 {
        42
    }

    /// 1-param function - always inlined
    fun double(x: u64): u64 {
        x + x
    }

    /// 2-param function - always inlined
    fun add(a: u64, b: u64): u64 {
        a + b
    }

    /// 3-param function - NOT inlined (exceeds limit)
    fun add3(a: u64, b: u64, c: u64): u64 {
        a + b + c
    }

    /// Nested call - calls another inlineable function
    fun quadruple(x: u64): u64 {
        double(double(x))
    }

    /// Bool param function - tests non-integral inlining
    fun negate(b: bool): bool {
        !b
    }

    /// Mixed params function - address + u64
    fun check_threshold(addr: address, value: u64): bool {
        addr != @0x0 && value > 100
    }
}
