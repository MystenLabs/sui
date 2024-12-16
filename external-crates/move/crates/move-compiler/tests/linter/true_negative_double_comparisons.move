module 0x42::M {

    fun test_true_negatives() {
        let x = 5;
        
        // Different variables being compared
        if (x == 10 || y < 10) {};
        
        // Non-overlapping ranges
        if (x > 20 || x < 10) {};
        
        // Single comparisons
        if (x <= 10) {};
        
        // Logical AND operations
        if (x > 0 && x < 10) {};
        
        // Different types of comparisons
        if (x == 10 || x != 20) {};
        
        // Complex expressions
        if (x + 1 == 10 || x - 1 < 10) {};
    }
}
