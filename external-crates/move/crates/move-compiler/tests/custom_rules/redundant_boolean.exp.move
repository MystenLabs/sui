module 0x42::M {
    // Function with non-redundant boolean expression
    public fun non_redundant_expression(y: bool): bool {
        y && true // Should not trigger a warning
    }

    // Function with redundant boolean expression
    public fun redundant_expression(y: bool): bool {
        true || y
    }

    // Function with redundant boolean expression
    public fun redundant_expression2(y: bool): bool {
        false || y // Should trigger a warning
    }
}