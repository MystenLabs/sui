address 0x42 {
module M {
    // has both prefix and postfix ability declarations
    // Postfix ability declarations are not supported before 2024 edition
    native struct Foo has copy, drop has store;
}
}
