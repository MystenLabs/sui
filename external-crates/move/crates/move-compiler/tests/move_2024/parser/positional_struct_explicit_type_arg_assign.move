module 0x42::M {
    public struct Foo<T>(T) has drop;

    fun should_fail() {
        Foo <u64>(_) = Foo(0);
    }
}
