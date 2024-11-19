module 0x42::M {
    struct S {}

    fun foo() {
        ERROR
        M::S {}
    }

    fun bar() {
        ERROR
        ERROR
        M::S {}
    }
}
