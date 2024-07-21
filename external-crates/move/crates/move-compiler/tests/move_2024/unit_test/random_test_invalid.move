module 0x1::l {
    public struct X() has drop;

    #[random_test]
    fun foo() { }

    #[random_test(b = 1)]
    fun go(_b: u64) { }

    #[random_test]
    #[test]
    fun qux(_c: u64, _d: bool) { }

    #[random_test]
    #[test]
    fun quxz() { }

    #[random_test]
    #[test_only]
    fun bar() { }


    #[random_test]
    fun baz(_: X) { }
}
