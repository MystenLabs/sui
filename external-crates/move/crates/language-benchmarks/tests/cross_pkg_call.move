// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Targets the pinned/system-package optimization from MystenLabs/sui#26508.
//
// `0x42::lib` is the callee package. When pinned via `MoveRuntime::new_with_system_packages`,
// the JIT translator rewrites cross-package calls into it as direct function pointers; when
// it is just a regular published package, those calls remain virtual and pay a vtable lookup
// per invocation. The `bench_*` functions below hammer those calls in a tight loop so the
// per-call dispatch cost dominates the iteration time.

module 0x42::lib {
    public fun noop() { }

    public fun add(x: u64, y: u64): u64 { x + y }

    public fun id<T>(x: T): T { x }
}

module 0x2::bench {
    const COUNT: u64 = 10_000;

    // Cross-pkg call with no args and no return — isolates raw dispatch overhead.
    public fun bench_cross_pkg_noop() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            0x42::lib::noop();
            i = i + 1;
        };
    }

    // Cross-pkg call with args and a return value.
    public fun bench_cross_pkg_add() {
        let mut acc: u64 = 0;
        let mut i: u64 = 0;
        while (i < COUNT) {
            acc = 0x42::lib::add(acc, i);
            i = i + 1;
        };
    }

    // Generic cross-pkg call — exercises the `CallGeneric` dispatch path, which carries the
    // same Direct/Virtual distinction that the optimization rewrites.
    public fun bench_cross_pkg_generic() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _ = 0x42::lib::id<u64>(i);
            i = i + 1;
        };
    }
}
