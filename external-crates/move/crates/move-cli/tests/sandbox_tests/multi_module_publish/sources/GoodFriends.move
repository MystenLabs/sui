module 0x2::A {
    public(package) fun foo() {}
}

module 0x2::B {
    #[allow(unused_function)]
    fun bar() { 0x2::A::foo() }
}
