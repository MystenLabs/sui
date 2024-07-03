module a::m {
    public struct None()

    macro fun ignore(
        _: None,
    ) {}

    fun t() {
        None().ignore!()
    }
}
