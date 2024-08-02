module 0x42::M {

    fun test_compound_shifts(x: u32) {
        let _a = x << (16 + 15);  // True negative: 31 is within bounds for u32
        let _b = x >> (16 * 2);   // True positive: 32 exceeds bounds for u32
    }
}
