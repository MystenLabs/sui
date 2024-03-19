address 0x42 {
module M {
    public struct Foo(u64) has copy, drop;

    fun x() {
        let x = Foo(0);
        let Foo(_) = x;
        Foo(_) = x;
        abort 0
    }
}
}


