module 0x42::A {
    struct A {}
}

module 0x42::M {
    friend 0x42::A::A;
}
