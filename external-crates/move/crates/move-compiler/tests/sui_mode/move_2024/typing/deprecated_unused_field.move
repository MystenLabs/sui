module a::m {
    #[deprecated]
    public struct A1(u64) has drop;
    // ^ Should not warn about unused field because type is marked deprecated

    #[allow(deprecated_usage)]
    public fun bad_1(): A1 { abort 0 }
}
