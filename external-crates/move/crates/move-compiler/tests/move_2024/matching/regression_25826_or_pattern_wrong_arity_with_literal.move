// Regression test for https://github.com/MystenLabs/sui/issues/25826
// Match compilation emitted a spurious ICE12003 ("Generated a failure
// expression, which should not be allowed under match exhaustion") when
// typing had already reported a wrong-arity error. The diagnostic should be
// suppressed when typing has reported errors, leaving only the original
// user-facing error.
module 0x42::m {
    public enum E has drop {
        A(u64),
        C(u64, u64, u64),
    }

    fun t(): u64 {
        let subject = E::A(0);
        match (subject) {
            E::A(x) if (x == &0) => x,
            E::A(x) | E::C(x, _, 0) | E::C(_, x) => x,
        }
    }
}
