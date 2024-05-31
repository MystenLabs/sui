module 0x42::TruePositiveTests {
    fun test_false_positive() {
        // Case 1: Multiplication followed by division
        let a: u64 = 18446744073709551615; // U64 MAX
        let b: u64 = 2;
        let c: u64 = 2;
        let _d = (a * b) / c; // Might trigger warning, but result is safe

        // Case 2: Multiplication with known small values in variables
        let e: u64 = 100;
        let f: u64 = 100;
        let _g = e * f; // Might trigger warning if the linter can't determine the actual values
    }
}
