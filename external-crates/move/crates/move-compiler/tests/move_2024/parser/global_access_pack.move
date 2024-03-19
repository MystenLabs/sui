module a::m {

    struct S { }

    fun foo(x: S) {
        ::a::m::S { } = x;
    }
}
