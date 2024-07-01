module 0x42::M {
    public fun nested_if_redundant(x: bool, y: bool) {
        if (x) {
            if (y) {
                
            };
        };
    }

    // This function combines conditions with &&, demonstrating the recommended approach
    public fun combined_conditions(x: bool, y: bool) {
        if (x && y) {
    
        };
    }
}
