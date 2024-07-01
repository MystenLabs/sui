module 0x42::M {
    const ERROR_NUM: u64 = 2;
    public fun func1(x: bool) {
        if (x == true) {};
        if (x == false) {};
        if (x != true) {};
        if (x != false) {};
        if (x == true || ERROR_NUM == 2) {};
        if (x == true && x != false) {};
        if (x) {};
        if (!x) {};
        if (true == x) {};
        if (condition() == true) {};
    }

    public fun non_redundant_expression(y: bool): bool {
        y && true // Should trigger a warning
    }

    // Function with redundant boolean expression
    public fun redundant_expression(y: bool): bool {
        true || y
    }

    // Function with redundant boolean expression
    public fun redundant_expression2(y: bool): bool {
        false || y // Should trigger a warning
    }

    fun condition(): bool {
        true
    }
}