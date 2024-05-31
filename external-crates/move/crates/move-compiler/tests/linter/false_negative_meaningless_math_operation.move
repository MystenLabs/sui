module 0x42::M {
     public fun complex_zero_operation(x: u64): u64 {
         x * (1 - 1) // This is effectively x * 0, but might not be caught
     }

     public fun complex_one_operation(x: u64): u64 {
         x * (2 - 1) // This is effectively x * 1, but might not be caught
     }

     public fun zero_shift_complex(x: u64, y: u64): u64 {
         x * (y - y) // This is effectively x << 0, but might not be caught
     }

    // Edge cases
    public fun divide_by_zero(x: u64): u64 {
        x / 0 // This is undefined behavior, should be caught by a different linter
    }

    public fun complex_zero_divide(x: u64): u64 {
        x / (1 - 1) // This is also undefined, might not be caught by this linter
    }
}
