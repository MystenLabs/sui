module 0x42::M {
    // This function should trigger the lint warning for `assert!(true)`
    public fun assert_true_redundant() {
        assert!(true, 0); // Hypothetical syntax, assuming `assert` takes a condition and an error code
    }

    // This function should trigger the lint warning for `assert!(false)`
    public fun assert_false_unreachable() {
        assert!(false, 1); // Hypothetical syntax, for demonstration
    }
}
