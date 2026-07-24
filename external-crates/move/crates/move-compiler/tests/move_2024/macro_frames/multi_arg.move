// Tests macro with multiple by-name parameters — sibling Argument frames.
module A::m {
    macro fun add($a: u64, $b: u64): u64 {
        $a + $b
    }

    public fun test(x: u64, y: u64): u64 {
        add!(x, y)
    }
}
