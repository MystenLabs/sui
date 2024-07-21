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

}
