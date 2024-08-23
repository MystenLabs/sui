module 0x42::M {
    public struct Foo(u16, u64) has copy, drop;
    public struct Bar()

    fun f(x: Foo) {
        Foo(t, y) = x;
        Foo() = x;
        Bar() = x;
        Bar(_, _) = x;
    }

    fun g(x: Foo) {
        let Foo() = x;
        let Bar() = x;
        let Bar(c, d) = x;
    }

    fun h(x: Bar) {
        Foo(_, _) = x;
        Foo(t, y) = x;
        Foo() = x;
        Bar(_, _) = x;
    }

    fun z(x: Bar) {
        let Foo(t, y) = x;
        let Foo() = x;
        let Bar(_, _) = x;
    }
}
