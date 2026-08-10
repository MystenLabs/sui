// Tests that argument evaluation frames appear in frame transitions.
// The argument `p + 1` is a multi-bytecode expression that should be
// colored with the Argument expansion color.
module A::m {
    fun identity(v: u64): u64 { v }

    macro fun process($x: u64): u64 {
        let base = identity(1);
        base + $x
    }

    public fun test(p: u64): u64 {
        process!(p + 1)
    }
}
