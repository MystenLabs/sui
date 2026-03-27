// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Minimal test for the color guard in eliminate_locals (cfgir/optimize).
//
// `let v = 1` lives in MacroBody(m). The lambda invocation `$f(v)` passes
// `v` across a scope boundary: Move(v) carries MacroBody(m) color.
// eliminate_locals removes `v` by substituting Value(1) at the use site.
// Without the color guard, Value(1) loses MacroBody(m) color and inherits
// Lambda from the command — making MacroBody(m) invisible as a separate
// frame transition.
module A::m {
    macro fun m($f: |u64| -> u64): u64 {
        let v = 1;
        $f(v)
    }

    public fun test(): u64 {
        m!(|x| x)
    }
}
