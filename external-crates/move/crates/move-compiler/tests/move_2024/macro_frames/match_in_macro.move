// Tests that compiler-generated match blocks inside a macro body stay
// in the MacroBody frame (match compilation creates blocks without an
// expansion color of their own, which inherit the color of their
// enclosing scope).
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
