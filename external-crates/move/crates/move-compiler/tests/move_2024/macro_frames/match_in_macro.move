// Tests that compiler-generated match blocks inside a macro body stay
// in the MacroBody frame (the compiler-generated blocks introduce no
// expansion of their own, so their instructions must remain attributed
// to the enclosing MacroBody frame).
module A::m {
    public enum E has drop {
        V(u64),
        Empty,
    }

    macro fun unwrap_or($e: E, $default: u64): u64 {
        match ($e) {
            E::V(x) => x,
            E::Empty => $default,
        }
    }

    public fun test(e: E): u64 {
        unwrap_or!(e, 42)
    }
}
