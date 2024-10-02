module 0x6::Bar {
    use 0x6::Foo;

    public bar() {
        Foo::foo();
    }
}
