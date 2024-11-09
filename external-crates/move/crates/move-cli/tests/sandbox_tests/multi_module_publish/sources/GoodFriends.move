module 0x7::A {
    public(package) fun foo() {}
}

module 0x7::B {
    #[allow(unused_function)]
    fun bar() { 0x7::A::foo() }
}
