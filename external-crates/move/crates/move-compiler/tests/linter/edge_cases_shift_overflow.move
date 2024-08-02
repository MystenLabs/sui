module 0x42::M {
    fun test_edge_cases(x: u64, y: u8) {
        let _a = x << (63 as u8); // True negative: maximum allowed shift for u64
        let _c = x << y;          // Potential false negative: cannot be statically determined
        let _d = x >> (y & 0x3F); // True negative: shift amount is always <= 63
        let _e = 1u128 << 127;    // True negative: literal shift within bounds
        let _f = 1u128 << 128;    // True positive: should raise an issue
    }
}
