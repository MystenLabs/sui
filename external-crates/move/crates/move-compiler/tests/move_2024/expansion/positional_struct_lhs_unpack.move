module 0x42::M {
    public struct Foo(u16, u64) has copy, drop;
    public struct Bar()

    fun f(x: Foo) {
        // Positional unpack of a struct. In expansion this gets moved from a a
        // `Call` expr to a `Unpack` expr.
        Foo(_, _) = x;
    }

    fun g(x: Bar) {
        // Positional unpack of a struct. In expansion this gets moved from a a
        // `Call` expr to a `Unpack` expr.
        Bar() = x;
    }
}
