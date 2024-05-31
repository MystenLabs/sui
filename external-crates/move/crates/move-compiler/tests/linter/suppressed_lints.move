module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(combinable_comparison))]
    public fun func1(x: u64, y: u64) {
        if (x < y || x == y) {}; // should be x <= y
    }
}
