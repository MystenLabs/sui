// Runtime check: `let ... else` inside a macro body. The body is inlined into
// the caller and the BindElse form must survive recoloring/expansion. The
// nested invocation exercises the recolor pass: two expansions in the same
// expression would collide on the macro's `x` binder if `BindElse`'s pattern
// binders weren't added to the recolor context.
//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    public fun c(x: u64): ABC<u64> { ABC::C(x) }

    macro fun unwrap_c($subj: ABC<u64>): u64 {
        let ABC::C(x) = $subj else { abort 0 };
        x
    }

    public fun call_macro(x: u64): u64 {
        unwrap_c!(c(x))
    }

    // nested invocation: the macro body's `x` binder is recolored separately
    // for each expansion, so two `let ABC::C(x) = ...` from the same macro
    // body can coexist in the same expression.
    public fun call_macro_nested(x: u64): u64 {
        unwrap_c!(c(unwrap_c!(c(x))))
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{call_macro, call_macro_nested};

        assert!(call_macro(7) == 7, 1);
        assert!(call_macro(0) == 0, 2);
        assert!(call_macro_nested(42) == 42, 3);
    }
}
