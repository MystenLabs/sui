module 0x42::M {
    public struct Foo(u64) has copy, drop;

    fun x() {
        // Error: Non-positional pack of positional struct
        let _x = Foo { pos0: 0 };
        abort 0
    }
}
