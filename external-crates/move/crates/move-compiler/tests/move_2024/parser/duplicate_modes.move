module a::m {
    #[mode(a,b,a)]
    fun err0() { }

    #[mode(a,b,a)]
    #[mode(b,a,b)]
    fun err1() { }
}
