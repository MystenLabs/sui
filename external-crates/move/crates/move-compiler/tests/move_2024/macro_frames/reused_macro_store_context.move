// Tests store attribution when local elimination moves a value from one
// invocation of a macro into a later invocation of the same macro. Although
// both MacroBody frames have the same source range, stores in the second
// invocation must retain the second frame's identity.
module A::m {
    macro fun m($x: u64): u64 {
        let y = $x;
        let _ = y + y;
        0
    }

    public fun test(v: u64): u64 {
        let x = m!(v);
        m!(x)
    }
}
