module 0x42::M {
    fun test_u8_shifts(x: u8) {
        let _a = x << 7;  // True negative: maximum allowed shift for u8
        let _b = x << 8;  // True positive: should raise an issue
        let _c = x << 9;  // True positive: should raise an issue
        let _d = x >> 7;  // True negative: maximum allowed shift for u8
        let _e = x >> 8;  // True positive: should raise an issue
    }

    fun test_u64_shifts(x: u64) {
        let _a = x << 63; // True negative: maximum allowed shift for u64
        let _b = x << 64; // True positive: should raise an issue
        let _c = x << 65; // True positive: should raise an issue
        let _d = x >> 63; // True negative: maximum allowed shift for u64
        let _e = x >> 64; // True positive: should raise an issue
    }

    fun test_u128_shifts(x: u128) {
        let _a = x << 127; // True negative: maximum allowed shift for u128
        let _b = x << 128; // True positive: should raise an issue
        let _c = x << 129; // True positive: should raise an issue
        let _d = x >> 127; // True negative: maximum allowed shift for u128
        let _e = x >> 128; // True positive: should raise an issue
    }
}
