module a::m {
    public struct X {}
    public fun foo(_: &X) {}

    #[test_only]
    use fun foo as X.f;
}
