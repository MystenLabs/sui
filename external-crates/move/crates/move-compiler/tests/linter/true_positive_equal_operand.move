module 0x42::true_positive_equal_operand {
    fun test_equal_operands_comparison(x: u64, b: bool) {
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
        b && b;
        // b || b;

        // x != x;
        // x < x;
        // x > x;

        // x == x;
        // x <= x;
        // x >= x;
    }
}
