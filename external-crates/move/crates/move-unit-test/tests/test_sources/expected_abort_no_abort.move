module 0x6::M {
    #[test, expected_failure]
    fun fail() { }

    #[test, expected_failure(abort_code=0, location=0x6::M)]
    fun fail_with_code() { }
}
