module Enums::nested_guard {
    public fun nested_guard(x: bool, b: bool): bool {
        match (x) {
            x if (match (b) { nested_var if (!*nested_var) => nested_var, z => z }) => x,
            _ => false
        }
    }
}
