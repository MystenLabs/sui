address 0x42 {
module M {
    // invalid ability declaration
    struct Foo has copy {} has;
}
}
