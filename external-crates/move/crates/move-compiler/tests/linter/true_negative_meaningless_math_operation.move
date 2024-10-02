module 0x42::M {
    public fun multiply_by_two(x: u64): u64 {
        x * 2 // Should not trigger the linter
    }

    public fun left_shift_by_one(x: u64): u64 {
        x << 1 // Should not trigger the linter
    }

    public fun add_one(x: u64): u64 {
        x + 1 // Should not trigger the linter
    }

    public fun divide_by_two(x: u64): u64 {
        x / 2 // Should not trigger the linter
    }
}
