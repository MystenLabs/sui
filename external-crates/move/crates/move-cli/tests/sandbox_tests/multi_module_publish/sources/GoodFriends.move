module 0x2::A {
    friend 0x2::B;
    public(friend) fun foo() {}
}

module 0x2::B {
    #[allow(unused_function)]
    fun bar() { 0x2::A::foo() }
}
