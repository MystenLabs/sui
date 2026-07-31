// Tests by-name argument substitution frame (Argument parent -> MacroBody).
module A::m {
    macro fun add_one($x: u64): u64 {
        $x + 1
    }

    public fun test(v: u64): u64 {
        add_one!(v)
    }
}
