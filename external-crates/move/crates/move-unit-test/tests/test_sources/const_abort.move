module 0x6::M {
    const ErrorCode: u64 = 42;

    const DifferentErrorCode: u64 = 42;

    #[test]
    #[expected_failure(abort_code = ErrorCode)]
    fun test_local_abort() {
        abort ErrorCode
    }

    #[test]
    #[expected_failure(abort_code = DifferentErrorCode)]
    fun test_local_abort_invalid_code() {
        abort ErrorCode
    }
}
