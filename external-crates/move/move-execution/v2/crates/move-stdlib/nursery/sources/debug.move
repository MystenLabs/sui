/// Module providing debug functionality.
module std::debug {

    /// Pretty-prints any Move value. For a Move struct, includes its field names, their types and their values.
    native public fun print<T>(x: &T);

    /// Prints the calling function's stack trace.
    native public fun print_stack_trace();

    #[test_only]
    use std::string;

    #[test_only]
    /// Utility function for printing a sequence of UTF8 bytes as a string (e.g., `b"Hello"`).
    public fun print_string(utf8_bytes: vector<u8>) {
        print<string::String>(&string::utf8(utf8_bytes));
    }
}
