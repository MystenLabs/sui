// `let <pat>: T = e else { ... }` — the type annotation flows through
// `BindElse` and the RHS is wrapped in an `Annotate` at expansion. This pins
// that path (no other positive test exercises it).
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T),
    }

    fun annotated(): u64 {
        let subject: ABC<u64> = ABC::C(42u64);
        let ABC::C(x): ABC<u64> = subject else { return 0 };
        x
    }

}
