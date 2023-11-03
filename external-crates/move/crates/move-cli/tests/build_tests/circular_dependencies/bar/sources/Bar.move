module 0x1::Bar {
    use 0x1::Foo;

    public bar() {
        Foo::foo();
    }
}
