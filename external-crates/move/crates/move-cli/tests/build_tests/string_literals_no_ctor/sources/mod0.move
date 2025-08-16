module std::ascii { public struct String has store, drop, copy {} }
module std::string { public struct String has store, drop, copy {} }

module a::m {
    public fun t0(): std::ascii::String { "hello" }
    public fun t1(): std::string::String { "hello" }
}
