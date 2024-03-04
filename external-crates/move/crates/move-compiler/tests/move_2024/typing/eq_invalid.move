module 0x8675309::M {
    public struct S { u: u64 }
    public struct R has key {
        f: u64
    }
    public struct G0<T> has drop { f: T }
    public struct G1<T: key> { f: T }
    public struct G2<phantom T> has drop {}


    fun t0(s: S, s_ref: &S, s_mut: &mut S) {
        (0: u8) == (1: u128);
        0 == false;
        &0 == 1;
        1 == &0;
        s == s_ref;
        s_mut == s;
    }

    fun t1(r: R) {
        r == r;
    }

    fun t3<T: copy + key>(t: T) {
        G0<R>{ f: R { f: 1 } } == G0<R>{ f: R { f: 1 } };
        // can be dropped, but cannot infer type
        G2{} == G2{};
        G1{ f: t } == G1{ f: t };
    }

    fun t4() {
        () == ();
        (0, 1) == (0, 1);
        (1, 2, 3) == (0, 1);
        (0, 1) == (1, 2, 3);
    }
}
