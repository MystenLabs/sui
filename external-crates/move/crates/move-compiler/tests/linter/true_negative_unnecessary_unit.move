// tests unnecessary units. These caeses are not errors and should not be reported
module a::unnecessary_unit {
    public fun t_if_without_else(cond: bool): u64 {
        let x = 0;
        if (cond) x = 1;
        x
    }

    public fun t() {
        () // unit here is okay
    }
}
