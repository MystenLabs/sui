module 0x42::M {

    // Control example: Correct use of `assert!` that shouldn't trigger the lint
    public fun assert_condition(x: bool) {
        assert!(x, 2); // Proper use of `assert` with a dynamic condition
    }

    // Another control example with a negated condition
    public fun assert_negated_condition(x: bool) {
        assert!(!x, 3); // Proper use of `assert` with a negated dynamic condition
    }
}
