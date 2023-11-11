address 0x42 {
module M {
    public struct Foo(u64)

    fun x(y: Foo): u64 {
        y.0_u8
    }
}
}

