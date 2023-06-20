module A::A {
    use A::Foo;

    #[allow(unused_function)]
    fun f(): u64 {
        Foo::foo()
    }
}
