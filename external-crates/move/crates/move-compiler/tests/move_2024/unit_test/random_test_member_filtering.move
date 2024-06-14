module 0x1::f {
    #[random_test]
    public fun f(_: u64) { }
}

module 0x1::g {
    public fun g() { 0x1::f::f(0); }
}
