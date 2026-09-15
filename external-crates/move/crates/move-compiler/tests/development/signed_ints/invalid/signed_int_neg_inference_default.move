module 0x42::m {
    // Unconstrained negation should default to i64
    fun neg_defaults_to_i64() {
        let _x = -1;
    }

    // Negation in expression context, no other type info
    fun neg_in_expr() {
        let _x = -(5);
    }

    // Negation of negation with no context. Use annotation to be deterministic
    fun double_neg_default() {
        let _x: i64 = -(-1);
    }

    // Negation with arithmetic, no context
    fun neg_arithmetic_default() {
        let _a = -1 + 2;
    }

    // Both operands negated
    fun neg_arithmetic_default2() {
        let _a: i64 = -1i64 * -2i64;
    }
}
