module 0x42::M {
    public fun nested_if_different_actions(x: bool, y: bool): bool {
        if (x) {
            if (y) {
                // Different action for y
                return true
            }
        };
        false
    }
}
