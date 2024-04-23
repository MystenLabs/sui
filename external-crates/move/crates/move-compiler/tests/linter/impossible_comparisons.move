module 0x42::M {

    fun func1(x: u64) {
        // This should trigger the lint: x cannot be both less than 5 and greater than 10.
        if (x < 5 && x > 10) {
            // Logic that can never be reached
        };
        if (x < 5 && 10 < x) {
            // Logic that can never be reached
        };
        if (x > 10 && x < 5) {
            // Logic that can never be reached
        };

        if (x > 5 && x < 10) {
            // Logic for when x is between 5 and 10
        };

        //This should trigger the lint: x cannot be both less than or equal to 2 and greater than or equal to 8.
        if (x <= 2 && x >= 8) {
            // Logic that can never be reached
        };

    }
    
    public fun possible_doubles(x: u64) {
        // These should not trigger the lint: It's possible for x to satisfy these conditions.
        if (x > 5 && x < 10) {
            // Logic for when x is between 5 and 10
        };

        // Using or (||) should not trigger the lint: It's asking if x is outside a range.
        if (x <= 5 || x >= 10) {
            // Logic for when x is not between 5 and 10
        };
    }
}