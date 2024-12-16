module 0x42::M {

    const MAX_U64: u64 = 18446744073709551615;
    const SPECIAL_VALUE: u64 = 42; 

    fun test_false_positives() {
        let x = 5;
        
        // Cases where simplification might lose precision due to overflow
        if (x == MAX_U64 || x > MAX_U64) {};  // Can't simplify due to overflow concerns
        
        // Cases with side effects in comparisons
        if (get_value() == 10 || get_value() < 10) {};  // Simplifying would change behavior
        
        // Cases where explicit checks might be preferred for readability
        if (x == SPECIAL_VALUE || x < SPECIAL_VALUE) {};  // Explicit check might be more meaningful
    }
}
