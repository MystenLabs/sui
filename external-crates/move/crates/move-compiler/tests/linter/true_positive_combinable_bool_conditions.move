module 0x42::true_positive_combinable_bool_conditions {
    fun test_true_positives() {
        let x = 10;
        let y = 20;

        // Case 1: x == y || x < y  should be x <= y
        if (x == y || x < y) {};

        // Case 2: x == y && x > y  is a contradiction (always false)
        if (x == y && x > y) {};

        // Case 3: x >= y && x == y  should be x == y
        if (x >= y && x == y) {};

        // Case 4: x == y || x > y  should be x >= y
        if (x == y || x > y) {};
    }
}
