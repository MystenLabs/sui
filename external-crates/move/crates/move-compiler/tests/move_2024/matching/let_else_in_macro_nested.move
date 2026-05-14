// Regression pin: two invocations of a `let ... else`-using macro in the same
// expression must each recolor the pattern's binders. Without registering the
// `BindElse` pattern's binders with the recolor context, the second expansion
// reuses the first's `x` and ICEs in typing with a duplicate-Var.
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun c(x: u64): ABC<u64> { ABC::C(x) }

    macro fun unwrap_c($subj: ABC<u64>): u64 {
        let ABC::C(x) = $subj else { abort 0 };
        x
    }

    fun nested(x: u64): u64 {
        unwrap_c!(c(unwrap_c!(c(x))))
    }

}
