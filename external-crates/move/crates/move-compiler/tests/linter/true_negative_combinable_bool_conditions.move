module 0x42::true_negative_combinable_bool_conditions {
    fun test_true_negatives() {
        let x = 10;
        let y = 20;

        // Case 1: Different variables in comparison
        if (x == y || z < y) {};

        // Case 2: Different operators that don't have simplification
        if (x > y || x < y) {};

        // Case 3: Complex expressions
        if ((x + 1) == y || (x - 1) < y) {};

        // Case 4: Non-combinable operators
        if (x != y && x > 0) {};
    }
}
