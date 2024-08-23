module 0x42::M {
    public struct Foo { field: u64 } has copy, drop;

    fun x() {
        let x = Foo { field: 0 };
        // Error: Positional unpack of non-positional struct
        let Foo(_) = x;
        abort 0
    }

    fun y() {
        let x = Foo { field: 0 };
        // Error: Positional unpack of non-positional struct
        Foo(_) = x;
        abort 0
    }
}
