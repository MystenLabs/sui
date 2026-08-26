// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Regression test for u256 value reconstruction in the trace adapter.
//
// A u256 is stored in the trace as its 32 little-endian bytes (see
// move-core-types `u256::to_le_bytes`), so the debugger must reconstruct the
// value with a per-byte (8-bit) shift. A per-word (64-bit) shift silently
// produces wildly wrong values (e.g. 256 -> 2^64), which this test pins.
module u256_values::m;

fun observe(small: u256, big: u256, max: u256): (u256, u256, u256) {
    // All three parameters are in scope at function entry; the test observes
    // them here. Returned as a tuple so none are reported unused (and so the
    // body performs no arithmetic that could overflow).
    (small, big, max)
}

#[test]
fun test() {
    let (_a, _b, _c) = observe(
        // 0x100 -> LE bytes [0, 1, 0, ...]; a 64-bit-word decode renders 2^64.
        256,
        // 2^64 + 1: exceeds u64 and spans a byte beyond the first 64-bit word.
        18446744073709551617,
        // 2^256 - 1: all 32 bytes are 0xFF; a 64-bit-word decode overflows nonsensically.
        115792089237316195423570985008687907853269984665640564039457584007913129639935,
    );
}
