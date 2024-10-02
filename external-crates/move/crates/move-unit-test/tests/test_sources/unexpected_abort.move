module 0x6::M {
    #[test]
    public fun unexpected_abort() {
        abort 0
    }

    #[test]
    #[expected_failure(abort_code=1, location=0x6::M)]
    public fun wrong_abort_code() {
        abort 0
    }

    #[test]
    #[expected_failure(abort_code=0, location=0x6::M)]
    public fun correct_abort_code() {
        abort 0
    }

    #[test]
    #[expected_failure]
    public fun just_test_failure() {
        abort 0
    }

    #[test_only]
    fun abort_in_other_function() {
        abort 1
    }

    #[test]
    fun unexpected_abort_in_other_function() {
        abort_in_other_function()
    }


    #[test]
    fun unexpected_abort_in_native_function() {
        abort_in_native()
    }

    fun abort_in_native() {
        std::string::internal_sub_string_for_testing(&vector[0], 1, 0);
    }
}
