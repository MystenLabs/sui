module 0x42::false_negative_combinable_bool_conditions {
    fun test_false_negatives() {
        let x = 10;
        let y = 20;

        // Case 1: Complex but equivalent expressions that could be simplified
        if ((x + 5) == (y - 5) || (x + 5) < (y - 5)) {};

        // Case 2: Reversed order of operands
        if (y > x || y == x) {};
    }

}
