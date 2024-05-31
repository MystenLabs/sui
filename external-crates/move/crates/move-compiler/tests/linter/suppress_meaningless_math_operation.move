module 0x42::M {

    #[allow(lint(meaningless_math_operation))]
    public fun add_zero(x: u64): u64 {
        x + 0 // Should trigger the linter
    }

    #[allow(lint(meaningless_math_operation))]
    public fun subtract_zero(x: u64): u64 {
        x - 0 // Should trigger the linter
    }
}
