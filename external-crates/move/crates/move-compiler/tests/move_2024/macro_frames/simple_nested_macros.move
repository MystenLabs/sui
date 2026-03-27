// Tests the frame gap: inner's MacroBody must have outer's MacroBody as its parent.
module A::m {
    macro fun inner($x: u64): u64 {
        $x + 1
    }

    macro fun outer($x: u64): u64 {
        inner!($x)
    }

    public fun test(v: u64): u64 {
        outer!(v)
    }
}
