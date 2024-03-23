module 0x42::M {

 public fun multiply_by_zero(x: u64): u64 {
        x * 0 // This should trigger the linter
    }

    public fun left_shift_by_zero(x: u64): u64 {
        x << 0 // This should trigger the linter
    }

    public fun right_shift_by_zero(x: u64): u64 {
        x >> 0 // This should trigger the linter
    }

    public fun multiply_by_one(x: u64): u64 {
        x * 1 // This should trigger the linter
    }

    public fun add_zero(x: u64): u64 {
        x + 0 // This should trigger the linter
    }

    public fun subtract_zero(x: u64): u64 {
        x - 0 // This should trigger the linter
    }

    // Intentionally meaningful operations for control cases
    public fun multiply(x: u64, y: u64): u64 {
        x * y // This should not trigger the linter
    }

    public fun divide_by_nonzero(x: u64, y: u64): u64 {
        x / y // Assuming y is not zero, this should not trigger the linter
    }

}