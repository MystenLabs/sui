module 0x43::C {
    struct T {}
}

module 0x42::B {
    use 0x43::C;
    public fun foo(): C::T {
        C::T {}
    }
    public fun bar(c: C::T) {
        let C::T {} = c;
    }
}
