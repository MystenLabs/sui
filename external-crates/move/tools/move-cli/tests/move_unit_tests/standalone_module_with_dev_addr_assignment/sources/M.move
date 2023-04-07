module A::M {
    #[test]
    fun nop() {}

    #[test]
    #[expected_failure]
    fun explicit_abort_expect_failure() {
        abort 42
    }
}
