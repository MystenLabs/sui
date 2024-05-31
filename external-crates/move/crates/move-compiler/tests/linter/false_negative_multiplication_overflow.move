module 0x42::TruePositiveTests {
    fun test_false_negative() {
        // Case 1: Multiplication within a complex expression
        let a: u64 = 18446744073709551615; // U64 MAX
        let b: u64 = 2;
        let d: u64 = 3;
        let _c = (d * 1) + (a * b); // The a * b part might be missed
    }
}
