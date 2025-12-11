// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Benchmarks focused on interpreter step() overhead.
///
/// These benchmarks are designed to measure the raw interpreter dispatch overhead
/// with minimal work per instruction. This helps isolate the cost of:
/// - Instruction fetch and dispatch
/// - Trace function calls (when tracing is disabled, should be ~0)
/// - Gas metering overhead
/// - Stack operations
///
/// Use these benchmarks to validate tracing optimizations by comparing:
/// - `cargo bench -p language-benchmarks -- interpreter_step` (without tracing)
/// - `cargo bench -p language-benchmarks --features move-vm-runtime/tracing -- interpreter_step` (with tracing)
module 0x1::bench {
    /// High iteration count to amortize setup costs and get stable measurements
    const ITERATIONS: u64 = 200_000;

    /// Main benchmark entry point - runs all sub-benchmarks
    public fun bench() {
        bench_step_minimal_loop();
        bench_step_load_constants();
        bench_step_locals();
        bench_step_arithmetic();
        bench_step_comparisons();
        bench_step_boolean();
        bench_step_bitwise();
        bench_step_branches();
        bench_step_mixed();
        bench_step_call_same_module();
        bench_step_call_cross_module();
    }

    /// Benchmark: Minimal instruction sequence
    /// Tests raw dispatch overhead with the simplest possible loop.
    /// Instructions per iteration: ~5 (LdU64, Lt, BrFalse, Add, Branch)
    fun bench_step_minimal_loop() {
        let mut i: u64 = 0;
        while (i < ITERATIONS) {
            i = i + 1;
        }
    }

    /// Benchmark: Load constants
    /// Tests LdU8, LdU64, LdTrue, LdFalse dispatch overhead.
    /// Heavy on constant loading instructions.
    fun bench_step_load_constants() {
        let mut i: u64 = 0;
        while (i < ITERATIONS) {
            let _a: u8 = 42;
            let _b: u64 = 12345678;
            let _c: bool = true;
            let _d: bool = false;
            i = i + 1;
        }
    }

    /// Benchmark: Local variable operations
    /// Tests CopyLoc, MoveLoc, StLoc dispatch overhead.
    /// Heavy on local variable manipulation.
    fun bench_step_locals() {
        let mut i: u64 = 0;
        let mut x: u64 = 1;
        let mut y: u64 = 2;
        while (i < ITERATIONS) {
            let temp = x;
            x = y;
            y = temp;
            i = i + 1;
        }
    }

    /// Benchmark: Arithmetic operations
    /// Tests Add, Sub, Mul, Div, Mod dispatch.
    /// Mix of arithmetic operations per iteration.
    fun bench_step_arithmetic() {
        let mut i: u64 = 0;
        let mut acc: u64 = 1;
        while (i < ITERATIONS) {
            acc = acc + i;
            acc = acc - 1;
            acc = (acc * 2) / 2;
            acc = acc % 1000000;
            i = i + 1;
        }
    }

    /// Benchmark: Comparison operations
    /// Tests Lt, Gt, Le, Ge, Eq, Neq dispatch.
    /// Heavy on comparison instructions.
    fun bench_step_comparisons() {
        let mut i: u64 = 0;
        let mut count: u64 = 0;
        while (i < ITERATIONS) {
            if (i < 100000) { count = count + 1; };
            if (i > 50000) { count = count + 1; };
            if (i <= 150000) { count = count + 1; };
            if (i >= 20000) { count = count + 1; };
            if (i == 100000) { count = count + 1; };
            if (i != 199999) { count = count + 1; };
            i = i + 1;
        }
    }

    /// Benchmark: Boolean operations
    /// Tests Or, And, Not dispatch.
    fun bench_step_boolean() {
        let mut i: u64 = 0;
        let mut result: bool = true;
        while (i < ITERATIONS) {
            let a = i < 100000;
            let b = i > 50000;
            result = (a && b) || (!a && !b);
            result = !result;
            i = i + 1;
        }
    }

    /// Benchmark: Bitwise operations
    /// Tests BitOr, BitAnd, Xor, Shl, Shr dispatch.
    fun bench_step_bitwise() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0xFFFF;
        while (i < ITERATIONS) {
            acc = acc | (i & 0xFF);
            acc = acc ^ (i >> 4);
            acc = (acc << 1) >> 1;
            i = i + 1;
        }
    }

    /// Benchmark: Branch-heavy code
    /// Tests BrTrue, BrFalse, Branch dispatch overhead.
    /// Many conditional branches per iteration.
    fun bench_step_branches() {
        let mut i: u64 = 0;
        let mut sum: u64 = 0;
        while (i < ITERATIONS) {
            if (i % 2 == 0) {
                sum = sum + 1;
            } else {
                sum = sum + 2;
            };
            if (i % 3 == 0) {
                sum = sum + 3;
            } else if (i % 3 == 1) {
                sum = sum + 4;
            } else {
                sum = sum + 5;
            };
            i = i + 1;
        }
    }

    /// Benchmark: Mixed instruction types
    /// Realistic mix of instructions similar to real Move code.
    /// Tests overall interpreter dispatch performance.
    fun bench_step_mixed() {
        let mut i: u64 = 0;
        let mut a: u64 = 1;
        let mut b: u64 = 1;
        let mut sum: u64 = 0;
        while (i < ITERATIONS) {
            // Fibonacci-like computation with various operations
            let temp = a + b;
            a = b;
            b = temp % 1000000; // Keep numbers bounded

            // Some comparisons and branches
            if (b > a) {
                sum = sum + b;
            } else {
                sum = sum + a;
            };

            // Bitwise operations
            sum = sum ^ (i & 0xFF);

            i = i + 1;
        }
    }

    /// Benchmark: Function calls within the same module
    /// Tests Call instruction dispatch overhead for intra-module calls.
    /// Each call does substantial work to ensure measurable time.
    fun bench_step_call_same_module() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < ITERATIONS) {
            acc = helper_compute(i, acc);
            i = i + 1;
        }
    }

    /// Helper function for same-module call benchmark.
    /// Does loads, stores, and arithmetic to ensure measurable work per call.
    fun helper_compute(x: u64, y: u64): u64 {
        // Multiple loads and stores
        let a = x + 1;
        let b = y + 2;
        let c = a + b;
        let d = c * 2;
        let e = d / 2;
        let f = e % 1000000;
        // More operations
        let g = f ^ (x & 0xFF);
        let h = g | (y & 0xFF);
        let result = h + a + b;
        result
    }

    /// Benchmark: Cross-module function calls
    /// Tests Call instruction dispatch overhead for inter-module calls.
    /// Each call does substantial work to ensure measurable time.
    fun bench_step_call_cross_module() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < ITERATIONS) {
            acc = 0x1::bench_xmodule::compute(i, acc);
            i = i + 1;
        }
    }
}

/// Helper module for cross-module call benchmarks
module 0x1::bench_xmodule {
    /// Compute function for cross-module call overhead testing.
    /// Does loads, stores, and arithmetic to ensure measurable work per call.
    public fun compute(x: u64, y: u64): u64 {
        // Multiple loads and stores
        let a = x + 1;
        let b = y + 2;
        let c = a + b;
        let d = c * 2;
        let e = d / 2;
        let f = e % 1000000;
        // More operations
        let g = f ^ (x & 0xFF);
        let h = g | (y & 0xFF);
        let result = h + a + b;
        result
    }
}
