// Regression test for https://github.com/MystenLabs/sui/issues/25825
// HLIR match-compilation panicked with `Option::unwrap() on None` when a
// match contained a wrong-arity unit-variant pattern in an `or` pattern with
// a literal arm. The wrong-arity is reported by typing; later passes must
// not crash on the resulting malformed AST.
module 0x42::m {
    public enum E<T> has drop {
        A,
        C(T, T, T),
    }

    fun t(): u64 {
        let subject = E::A(0);
        match (subject) {
            E::A(x) if (true) => x,
            E::A(x) | E::C(x, _, 0) => x,
            _ => 1,
        }
    }
}
