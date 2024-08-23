module 0x42::m {
    #[deprecated = b"This is a deprecated function"]
    fun f() { }

    #[deprecated(msg = b"This is a deprecated function")]
    fun g() { }

    #[deprecated(b"This is a deprecated function")]
    fun h() { }

    #[deprecated(note = b"This is a deprecated function", other = b"other")]
    fun i() { }

    #[deprecated(note = 123)]
    fun j() { }

    #[deprication]
    fun k() { }

    #[deprecated(foo)]
    fun l() { }
}
