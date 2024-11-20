// emit a warning during code generation for equal operands in binary operations that result
// in a constant value
module a::m {
    fun test_equal_operands_comparison(x: u64, _b: bool) {
        1 == 1;
        // (*&1) == 1;
        // &1 == 1;
        // {1} == 1;
        // *&x == copy x;

        // x - x;
        // x % x;
        // x ^ x;
        // x / x;

        x | x;
        // x & x;
        // b && b;
        // b || b;

        // x != x;
        // x < x;
        // x > x;

        // x == x;
        // x <= x;
        // x >= x;
    }
}
