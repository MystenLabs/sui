module a::m {
    public struct None()

    macro fun ignore(
        _: None,
        _: ||,
        $_n: None,
        $_f: ||,
    ) {}

    fun t() {
        ignore!(None(), || (), None(), || ());
    }
}
