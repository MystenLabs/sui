module a::m {
    struct X {}
    public fun foo(_: &X) {}

    #[test_only]
    use fun foo as X.f;
}
