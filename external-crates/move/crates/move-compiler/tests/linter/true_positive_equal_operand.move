module 0x42::true_positive_equal_operand {
    fun test_equal_operands_comparison() {
        let x = 10;
        
        // Comparison operators
        let _ = x == x;  // lint warning expected
        let _ = x != x;  // lint warning expected
        let _ = x > x;   // lint warning expected
        let _ = x >= x;  // lint warning expected
        let _ = x < x;   // lint warning expected
        let _ = x <= x;  // lint warning expected

        // Logical operators
        let _ = true && true;    // lint warning expected
        let _ = false || false;  // lint warning expected

        // Bitwise operators
        let _ = x & x;   // lint warning expected
        let _ = x | x;   // lint warning expected
        let _ = x ^ x;   // lint warning expected

        // Arithmetic operators
        let _ = x - x;   // lint warning expected
        let _ = x / x;   // lint warning expected
    }
}
