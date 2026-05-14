// Runtime check: each let-else pattern shape introduces a set of binders that
// must survive into the success arm. Covers `@`-pattern (outer binder), `..`
// ellipsis (field and positional), multi-binder, `mut` binder, and shadowing
// across consecutive let-else bindings.
//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has copy, drop {
        A(T),
        B,
        C(T)
    }

    public enum FE<T> has drop {
        S { x: T, y: T, z: T },
        N,
    }

    public enum PE<T> has drop {
        S(T, T, T),
        N,
    }

    public enum Triple<T> has drop {
        T(T, T, T),
        N,
    }

    public enum O<T> has drop {
        S(T),
        N,
    }

    public fun a(x: u64): ABC<u64> { ABC::A(x) }
    public fun b(): ABC<u64> { ABC::B }
    public fun c(x: u64): ABC<u64> { ABC::C(x) }

    public fun fe_s(x: u64, y: u64, z: u64): FE<u64> { FE::S { x, y, z } }
    public fun fe_n(): FE<u64> { FE::N }

    public fun pe_s(x: u64, y: u64, z: u64): PE<u64> { PE::S(x, y, z) }
    public fun pe_n(): PE<u64> { PE::N }

    public fun triple(a: u64, b: u64, c: u64): Triple<u64> { Triple::T(a, b, c) }
    public fun triple_n(): Triple<u64> { Triple::N }

    // at-pattern: outer binder gets the whole constructor value. We confirm
    // that by destructuring `v` a second time and returning its inner field.
    // Sentinel 999 distinguishes the else branch.
    public fun at_pattern_then_inner(subject: ABC<u64>): u64 {
        let v @ ABC::C(_) = subject else { return 999 };
        let ABC::C(inner) = v else { return 998 };
        inner
    }

    // field ellipsis: only a subset of fields is bound; rest skipped.
    public fun field_ellipsis(e: FE<u64>): u64 {
        let FE::S { x, .. } = e else { return 0 };
        x
    }

    // positional ellipsis: only a subset of positions is bound; rest skipped.
    public fun positional_ellipsis(e: PE<u64>): u64 {
        let PE::S(x, ..) = e else { return 0 };
        x
    }

    // multiple binders introduced in one pattern, all visible after.
    public fun multi_binder(t: Triple<u64>): u64 {
        let Triple::T(a, b, c) = t else { return 0 };
        a + b + c
    }

    // mut binder: the introduced local is reassignable.
    public fun mut_binder(): u64 {
        let subject = ABC::C(10u64);
        let ABC::C(mut x) = subject else { abort 0 };
        x = x + 1;
        x
    }

    // same name reused across two consecutive let-elses; the second shadows.
    public fun shadowing(): u64 {
        let O::S(x) = O::S(1u64) else { return 0 };
        let O::S(x) = O::S(x + 10) else { return 0 };
        x
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{
            a, b, c, fe_s, fe_n, pe_s, pe_n, triple, triple_n,
            at_pattern_then_inner, field_ellipsis, positional_ellipsis,
            multi_binder, mut_binder, shadowing,
        };

        // at-pattern: outer binder holds the whole constructor value
        assert!(at_pattern_then_inner(c(7)) == 7, 1);
        assert!(at_pattern_then_inner(b()) == 999, 2);
        assert!(at_pattern_then_inner(a(5)) == 999, 3);

        // field ellipsis
        assert!(field_ellipsis(fe_s(10, 20, 30)) == 10, 4);
        assert!(field_ellipsis(fe_n()) == 0, 5);

        // positional ellipsis
        assert!(positional_ellipsis(pe_s(11, 22, 33)) == 11, 6);
        assert!(positional_ellipsis(pe_n()) == 0, 7);

        // multi-binder
        assert!(multi_binder(triple(1, 2, 3)) == 6, 8);
        assert!(multi_binder(triple_n()) == 0, 9);

        // mut binder
        assert!(mut_binder() == 11, 10);

        // shadowing
        assert!(shadowing() == 11, 11);
    }
}
