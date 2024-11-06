module 0x42::true_negative_equal_operand {
    fun test_equal_operands_comparison() {
        let x = 10;
        let y = 20;

        // Different operands
        let _ = x == y;
        let _ = x != y;
        let _ = x > y;
        let _ = x >= y;
        let _ = x < y;
        let _ = x <= y;

        // Valid arithmetic operations
        let _ = x + y;  // Addition is not checked
        let _ = x * y;  // Multiplication is not checked
        let _ = x % y;  // Modulo is not checked

        // Different values
        let _ = true && false;
        let _ = false || true;
        let _ = 5 & 3;
        let _ = 5 | 3;
        let _ = 5 ^ 3;
    }
}
