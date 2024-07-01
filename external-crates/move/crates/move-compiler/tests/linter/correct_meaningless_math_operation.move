module 0x42::M {
    // Intentionally meaningful operations for control cases
    public fun multiply(x: u64, y: u64): u64 {
        x * y // This should not trigger the linter
    }

    public fun divide_by_nonzero(x: u64, y: u64): u64 {
        x / y // Assuming y is not zero, this should not trigger the linter
    }
}
