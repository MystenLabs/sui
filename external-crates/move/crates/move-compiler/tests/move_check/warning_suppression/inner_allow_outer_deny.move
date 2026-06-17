// Test that inner #[allow] overrides outer #[deny] for the same code.
#[deny(unused_variable)]
module 0x42::m {
    #[allow(unused_variable)]
    fun allowed(a: u64) {
        let x;
    }

    // This function has no inner allow, so deny should apply.
    fun denied(a: u64) {
        let y;
    }
}
