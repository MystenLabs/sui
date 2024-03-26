module 0x42::M {
    public fun nested_if_redundant(x: bool, y: bool): bool {
        if (x) {
            if (y) {
                // Action when both x and y are true
                return true;
            }
        };
        false
    }

    // This function combines conditions with &&, demonstrating the recommended approach
    public fun combined_conditions(x: bool, y: bool): bool {
        if (x && y) {
            // Action when both x and y are true
            return true;
        }
        false
    }

    // Control example: Nested `if` with different actions, which should not trigger the lint
    public fun nested_if_different_actions(x: bool, y: bool): bool {
        if (x) {
            x = false;
            if (y) {
                // Different action for y
                return true;
            }
        }
        false
    }
}