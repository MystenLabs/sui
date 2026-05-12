// Runtime check: pure grammar surfaces of `let ... else`. Each form here is
// something the parser/expansion must accept; the test asserts that codegen
// and execution also behave correctly. Covers `|`-patterns, `_` wildcards,
// bare-expression `else`, type annotations on the pattern, unit-variant
// patterns with no binders, and visibility of outer locals inside `else`.
//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T),
    }

    public fun a(x: u64): ABC<u64> { ABC::A(x) }
    public fun b(): ABC<u64> { ABC::B }
    public fun c(x: u64): ABC<u64> { ABC::C(x) }

    // or-pattern: matches multiple variants binding the same name
    public fun or_pattern(subject: ABC<u64>): u64 {
        let ABC::C(x) | ABC::A(x) = subject else { return 0 };
        x
    }

    // wildcard binder inside the pattern
    public fun wildcard_inner(subject: ABC<u64>): u64 {
        let ABC::C(_) = subject else { return 0 };
        1
    }

    // bare-expression else (no braces)
    public fun non_block_else(subject: ABC<u64>): u64 {
        let ABC::C(x) = subject else return 0;
        x
    }

    // type annotation on the pattern: flows through BindElse and wraps the
    // RHS in an Annotate at expansion.
    public fun type_annotation(subject: ABC<u64>): u64 {
        let ABC::C(x): ABC<u64> = subject else { return 0 };
        x
    }

    // pattern introduces no binders (unit variant)
    public fun no_binders(subject: ABC<u64>): bool {
        let ABC::B = subject else { return false };
        true
    }

    // outer locals visible inside the else block
    public fun outer_in_else(subject: ABC<u64>): u64 {
        let fallback = 99u64;
        let ABC::C(x) = subject else { return fallback };
        x
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{
            a, b, c,
            or_pattern, wildcard_inner, non_block_else,
            type_annotation, no_binders, outer_in_else,
        };

        // or-pattern
        assert!(or_pattern(a(7)) == 7, 1);
        assert!(or_pattern(c(8)) == 8, 2);
        assert!(or_pattern(b()) == 0, 3);

        // wildcard
        assert!(wildcard_inner(c(5)) == 1, 4);
        assert!(wildcard_inner(a(5)) == 0, 5);
        assert!(wildcard_inner(b()) == 0, 6);

        // bare-expression else
        assert!(non_block_else(c(42)) == 42, 7);
        assert!(non_block_else(b()) == 0, 8);

        // type annotation
        assert!(type_annotation(c(13)) == 13, 9);
        assert!(type_annotation(a(13)) == 0, 10);

        // no binders
        assert!(no_binders(b()) == true, 11);
        assert!(no_binders(c(0)) == false, 12);

        // outer scope visible in else
        assert!(outer_in_else(c(7)) == 7, 13);
        assert!(outer_in_else(b()) == 99, 14);
    }
}
