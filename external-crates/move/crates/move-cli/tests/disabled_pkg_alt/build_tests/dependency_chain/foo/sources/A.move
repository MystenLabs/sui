module A::Foo {
    use A::Bar;

    public fun foo(): u64 {
        Bar::bar()
    }
}
