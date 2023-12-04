module B::Bar {
    use C::Foo;

    public fun foo(): u64 {
        Foo::bar()
    }
}
