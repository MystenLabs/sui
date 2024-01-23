module a::m {
    // invalid cycle through lambda
    macro fun foo(f: || u64): u64 {
        bar!(|| foo!(f))
    }

    macro fun bar(f: || u64): u64 {
        foo!(|| bar!(f))
    }
    fun t() {
        foo!(|| 0);
    }
}
