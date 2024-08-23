// tests suppression of unnecessary math operations
module 0x42::M {

    #[allow(lint(unnecessary_math))]
    public fun add_zero(x: u64): u64 {
        x + 0
    }
}
