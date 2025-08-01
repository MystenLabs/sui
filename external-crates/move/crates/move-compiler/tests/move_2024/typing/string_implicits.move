module a::m {
    fun test_0(): std::ascii::String { std::ascii::string("test") }
    fun test_1(): std::ascii::String { "test" }
    fun test_2(): std::string::String { std::string::utf8("test") }
    fun test_3(): std::string::String { "test" }
}
