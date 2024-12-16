module 0x42::M {

    fun test_false_negatives() {
        let x = 5;
        
        // Complex equivalent expressions
        if (x + 5 == 15 || x + 5 < 15) {};  // Should detect this as equivalent to (x + 5) <= 15
        
        // Nested comparisons
        if ((x == 10 || x < 10) || y == 20) {};  // Should still detect the x comparison
        
        // Non-numeric comparisons that follow the same logic
        if (string == "a" || string < "a") {};  // Same logical pattern but with strings
        
        // Equivalent comparisons with different variable arrangements
        if (10 == x || 10 > x) {};  // Should detect despite variable position swap
    }
}
