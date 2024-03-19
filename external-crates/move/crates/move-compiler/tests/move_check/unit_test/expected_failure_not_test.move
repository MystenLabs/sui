// expected_failure attributes can only be placed on #[test] functions
module 0x1::A {
    #[expected_failure]
    fun foo() { }

    #[test_only, expected_failure]
    fun bar() { }
}
