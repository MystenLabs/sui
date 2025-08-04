module std::ascii { public struct String {} }
module std::string { public struct String {} }
module a::m {
    fun t0(): std::ascii::String { "hello" }
    fun t1(): std::string::String { "hello" }
}

