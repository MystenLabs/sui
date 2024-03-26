module 0x42::M {
    // This function should trigger the lint warning for `assert!(true)`
    public fun assert_true_redundant() {
        assert!(true, 0); // Hypothetical syntax, assuming `assert` takes a condition and an error code
    }

    // This function should trigger the lint warning for `assert!(false)`
    public fun assert_false_unreachable() {
        assert!(false, 1); // Hypothetical syntax, for demonstration
    }

    // // Control example: Correct use of `assert!` that shouldn't trigger the lint
    public fun assert_condition(x: bool) {
        assert!(x, 2); // Proper use of `assert` with a dynamic condition
    }

    // // Another control example with a negated condition
    public fun assert_negated_condition(x: bool) {
        assert!(!x, 3); // Proper use of `assert` with a negated dynamic condition
    }
}