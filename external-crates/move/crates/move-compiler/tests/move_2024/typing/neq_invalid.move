module 0x8675309::M {
    public struct S has drop { u: u64 }
    public struct R { f: u64 }
    public struct G0<phantom T> {}
    public struct G1<phantom T: key> {}
    public struct G2<phantom T> has drop {}

    fun t0(s: S, s_ref: &S, s_mut: &mut S) {
        (0: u8) != (1: u128);
        0 != false;
        &0 != 1;
        1 != &0;
        s != s_ref;
        s_mut != s;
    }

    fun t1(r: R) {
        r != r;
    }

    fun t3() {
        G0{} != G0{};
        G1{} != G1{};
        G2{} != G2{};
    }

    fun t4() {
        () != ();
        (0, 1) != (0, 1);
        (1, 2, 3) != (0, 1);
        (0, 1) != (1, 2, 3);
    }
}
