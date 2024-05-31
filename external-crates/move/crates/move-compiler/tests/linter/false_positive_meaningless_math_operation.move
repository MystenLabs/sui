module 0x42::M {
   public fun multiply_by_zero_var(x: u64, y: u64): u64 {
        x * y // Might trigger if y is always 0, but shouldn't if y is variable
    }

    public fun add_zero_var(x: u64, y: u64): u64 {
        x + y // Might trigger if y is always 0, but shouldn't if y is variable
    }
}
