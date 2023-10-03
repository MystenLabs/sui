address 0x42 {
module M {
    // Valid positional field struct declaration
    public struct Foo(x: u64) has copy, drop;
}
}

