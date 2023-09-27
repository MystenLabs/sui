address 0x42 {
module M {
    // has both prefix and postfix ability declarations
    struct Foo has copy, drop {} has store;
}
}
