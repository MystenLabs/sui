address 0x42 {
module M {
    // has both prefix and postfix ability declarations
    public struct Foo has copy, drop {} has store;
}
}
