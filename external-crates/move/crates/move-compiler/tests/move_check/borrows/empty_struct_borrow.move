module 0x42::m {
    struct A { }

    fun foo(a: A): u8 {
        let A { } = &a;
        let A { } = a;
        0
    }

}
