address 0x42 {
module M {
    // Ability declarations require commas between them (this will error before hitting this though).
    // Postfix ability declarations are not supported before 2024 edition
    struct Foo {} has store copy;
}
}
