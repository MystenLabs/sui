address 0x42 {
module M {
    // Invalid type inside a positional struct
    public struct Foo(Not a type)
}
}
