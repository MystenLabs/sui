module 0x1::l {
    public struct X() has drop;

    #[rand_test]
    fun foo() { }

    #[rand_test(b = 1)]
    fun go(_b: u64) { }

    #[rand_test]
    #[test]
    fun qux(_c: u64, _d: bool) { }

    #[rand_test]
    #[test]
    fun quxz() { }

    #[rand_test]
    #[test_only]
    fun bar() { }


    #[rand_test]
    fun baz(_: X) { }
}
