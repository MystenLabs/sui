// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Regression test for u128 value reconstruction in the trace adapter.
//
// A u128 is serialized in the trace as a plain JSON number. Values above 2^53 cannot be represented
// exactly by a JavaScript double, so the adapter must parse them losslessly (via the JSON
// source-text reviver); otherwise `JSON.parse` silently rounds them.
module u128_values::m;

fun observe(small: u128, big: u128, max: u128): (u128, u128, u128) {
    // All three parameters are in scope at function entry; the test observes them here.
    (small, big, max)
}

#[test]
fun test() {
    let (_a, _b, _c) = observe(
        // 2^53 + 1: the smallest integer a JS double rounds (down to 2^53).
        9007199254740993,
        // u64::MAX: rounds to 18446744073709552000 as a double.
        18446744073709551615,
        // u128::MAX: rounds to ~3.4e38 as a double.
        340282366920938463463374607431768211455,
    );
}
