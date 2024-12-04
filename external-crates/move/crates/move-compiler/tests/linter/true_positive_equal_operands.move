// warn on operands that are always equal and the binary operation results in a constant value
// this excludes expressions that are folded by the compiler

module a::m {
    fun test_equal_operand(x: u64) {
        x % copy x;
        copy x ^ x;
        x / x;

        x | x;
        x & x;

        x != x;
        x < x;
        x > x;

        x == x;
        x <= x;
        copy x >= move x;
    }
    fun all_types<T: copy + drop>(x: T) {
        x == x;
    }
}
