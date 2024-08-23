module 0x42::m {
    #[deprecated]
    #[deprecated]
    fun foo() {}

    #[deprecated(note = b"note")]
    #[deprecated]
    fun bar() {}

    #[deprecated, deprecated(note = b"note")]
    fun baz() {}

    #[deprecated, deprecated]
    fun qux() {}
}
