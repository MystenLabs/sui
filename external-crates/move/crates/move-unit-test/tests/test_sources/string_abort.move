module 0x6::M {
    #[test]
    fun unexpected_abort_in_native_function() {
        abort_in_native()
    }

    fun abort_in_native() {
        std::string::internal_sub_string_for_testing(&vector[0], 1, 0);
    }
}
