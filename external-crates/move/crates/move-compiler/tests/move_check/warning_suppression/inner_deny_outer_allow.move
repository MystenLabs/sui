// Test that inner #[deny] overrides outer #[allow] for the same code.
#[allow(unused_variable)]
module 0x42::m {
    #[deny(unused_variable)]
    fun denied(a: u64) {
        let x;
    }

    // This function inherits the outer allow, so no warning.
    fun allowed(a: u64) {
        let y;
    }
}
