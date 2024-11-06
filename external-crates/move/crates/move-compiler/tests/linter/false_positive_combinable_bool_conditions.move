module 0x42::false_positive_combinable_bool_conditions {
    fun test_false_positives() {
        let x = 10;
        let y = 20;

        // Case 1: When order matters due to side effects
        if (get_value() == y || get_value() < y) {};

        // Case 2: When precision matters in floating point comparisons
        if (x as u64 == y || x as u64 < y) {};
    }
}
