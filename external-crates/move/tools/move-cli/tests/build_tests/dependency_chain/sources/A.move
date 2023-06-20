module A::A {
    use A::Foo;

    fun f(): u64 {
        Foo::foo()
    }
}
