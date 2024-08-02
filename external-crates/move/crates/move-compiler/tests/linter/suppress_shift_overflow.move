module 0x42::M {

    #[allow(lint(shift_overflow))]
    fun test_suppressions(x: u64) {
        let _a = x << 64;  // Suppressed: should not raise an issue despite overflow
        let _f = x << 128;  // Suppressed: demonstrates multiple lint suppressions
    }
}
