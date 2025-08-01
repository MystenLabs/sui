module a::m {
    fun test0(): std::ascii::String { "asdf\x80" }
    fun test1(): std::ascii::String { "\x80asdf" }
    fun test2(): std::ascii::String { "asdf\x80asdf" }
}

