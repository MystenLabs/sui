module 0x2::X {
    public fun u() {}
}

module 0x2::M {
    use 0x2::X::{Self, u as X};
    fun foo() {
        X();
        X::u()
    }
}
