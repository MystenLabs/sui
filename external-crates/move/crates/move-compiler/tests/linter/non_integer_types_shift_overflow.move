module 0x42::M {
    fun test_non_integer_types(b: bool, addr: address) {
        let _a = b << 1;  // Should not raise an issue (type mismatch, caught by compiler)
        let _b = addr >> 2; // Should not raise an issue (type mismatch, caught by compiler)
    }
}
