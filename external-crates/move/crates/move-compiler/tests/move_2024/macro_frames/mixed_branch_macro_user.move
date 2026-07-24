// Tests a branch where only one arm enters a macro expansion. This keeps the
// control-flow shape small while exercising transitions between macro-colored
// bytecode and user bytecode in the same expression.
module A::m {
    macro fun id($x: u64): u64 {
        $x
    }

    public fun test(b: bool, v: u64): u64 {
        if (b) {
            id!(v)
        } else {
            v + 1
        }
    }
}
