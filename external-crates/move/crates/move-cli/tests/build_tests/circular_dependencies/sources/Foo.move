module 0x6::Foo {
    use 0x6::Bar;

    public foo() {
        Bar::bar();
    }
}
