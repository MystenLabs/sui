module a::m {

    struct S {}

    fun make_s(): S { S { } }

    fun test() {
        let _s0 = make_s();
        let _s1 = make_s();
        abort 0x00F
    }
}
