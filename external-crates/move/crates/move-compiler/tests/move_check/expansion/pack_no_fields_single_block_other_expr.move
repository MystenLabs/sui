module 0x42::M {
    struct S { f: u64 }
    struct G has drop {}
    fun foo() {
        let _f = 0;
        let _s = S 0;
        let _s = S f;
        let _g = G ();
        let _g = G { {} };
    }
}
