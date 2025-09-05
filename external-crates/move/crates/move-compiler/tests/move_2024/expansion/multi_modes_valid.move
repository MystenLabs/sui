module a::m {
    #[mode(a)]
    #[mode(b)]
    fun valid1() { }

    #[mode(a,b)]
    fun valid2() { }

    #[mode(a)]
    #[test]
    fun valid6() { }

    #[mode()]
    fun valid7() { }
}
