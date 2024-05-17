// NOTE: the expected file does not demonstrate the autocomplete occurring, but debug_print calls
// showcase the behavior as working.

module a::m {

    public struct A has copy, drop {
        x: u64
    }

    public struct B has copy, drop {
        a: A
    }

    fun foo() {
        let _s = B { a: A { x: 0 } };
        let _tmp2 = _s.b.x;
    }
}
