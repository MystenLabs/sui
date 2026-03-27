// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests the other branch of the eliminate_locals color guard:
// replacement.color is non-None (Argument), so the guard does NOT fire.
// `outer` passes `1` to `m` via `$v`; the by-name substitution gives
// Value(1) an Argument color. The guard correctly preserves it.
module A::m {
    macro fun m($v: u64, $f: |u64| -> u64): u64 {
        let v = $v;
        $f(v)
    }

    macro fun outer($g: |u64| -> u64): u64 {
        m!(1, $g)
    }

    public fun test(): u64 {
        outer!(|x| x)
    }
}
