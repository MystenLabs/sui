module 0x42::TruePositiveTests {
    fun test_true_negative() {
        // Case 1: Safe U64 multiplication
        let a: u64 = 1000000;
        let b: u64 = 1000;
        let _c = a * b; // Should not trigger warning

        // Case 2: Safe U128 multiplication
        let d: u128 = 1000000000000000000;
        let e: u128 = 1000000;
        let _f = d * e; // Should not trigger warning
    }
}
