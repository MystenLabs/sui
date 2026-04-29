// Regression test for https://github.com/MystenLabs/sui/issues/26110
// HLIR `bind_local` panicked with `assert!(cur_t == &t)` when an `or` pattern
// rebound the same name across branches whose types disagreed because typing
// had already reported a wrong-arity diagnostic. The compiler should report
// the underlying error without an ICE.
module 0x42::m {
    public enum E<T> {
        A(T),
        B,
        C(T, T),
    }
    fun t(): u64 {
        match (E::A(0)) {
            E::C(x, (0 | 1)) | E::B(x) | E::A(x) => x,
            _ => 1,
        }
    }
}
