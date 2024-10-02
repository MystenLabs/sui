module 0x42::M {
    public struct Foo(u64) has copy, drop;

    fun x() {
        let x = Foo(0);
        // Error: Non-positional unpack of positional struct
        let Foo { y: _ } = Foo(0);
        abort 0
    }

    fun y() {
        let x = Foo(0);
        // Error: Non-positional unpack of positional struct
        Foo { y: _ } = Foo(0);
        abort 0
    }
}
