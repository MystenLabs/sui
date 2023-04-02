address 0x1 {
module M {
    #[test, expected_failure]
    fun fail() { }

    #[test, expected_failure(abort_code=0, location=0x1::M)]
    fun fail_with_code() { }
}
}
