module a::m {
    fun test0(): std::ascii::String { "asdfΓ" }
    fun test1(): std::ascii::String { "Γasdf" }
    fun test2(): std::ascii::String { "asdfΓasdf" }
}
