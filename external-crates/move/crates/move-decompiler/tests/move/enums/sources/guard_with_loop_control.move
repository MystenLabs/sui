// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The compiler accepts loop control inside match guards. The structurer cannot keep such a
// guard inside the arm, so these decompile to the no-guard fallback shape; these fixtures
// pin that behavior (and that guard recovery declines rather than missplitting).
module enums::guard_with_loop_control;

public enum E has copy, drop {
    A { x: u64 },
    B,
}

public fun break_in_guard(e: &E): u64 {
    let mut acc = 0;
    loop {
        match (e) {
            E::A { x } if ({ if (*x > 10) break; *x > 0 }) => { acc = acc + 1; },
            _ => break,
        }
    };
    acc
}

public fun continue_in_guard(e: &E): u64 {
    let mut acc = 0;
    let mut i = 0;
    while (i < 20) {
        i = i + 1;
        match (e) {
            E::A { x } if ({ if (*x == 0) continue; *x > 0 }) => { acc = acc + 1; },
            _ => {},
        }
    };
    acc
}

public fun labeled_break_in_guard(e: &E): u64 {
    let mut acc = 0;
    'outer: loop {
        loop {
            match (e) {
                E::A { x } if ({ if (*x > 10) break 'outer; *x > 0 }) => { acc = acc + 1; },
                _ => break 'outer,
            }
        }
    };
    acc
}
