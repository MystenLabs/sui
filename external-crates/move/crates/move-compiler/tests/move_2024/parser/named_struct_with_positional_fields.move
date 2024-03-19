address 0x42 {
module M {
    // Invalid positional field struct declaration
    public struct Foo{u64, u16} has copy, drop;
}
}
