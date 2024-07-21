module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(meaningless_math_op))]
    public fun subtract_zero(x: u64): u64 {
        x - 0 // This should trigger the linter
    }
}
