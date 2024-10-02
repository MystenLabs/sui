// module member aliases do not shadow leading access names
module a::S {
    public struct S() has copy, drop;
    public fun foo() {}
}

module a::with_struct {
    const S: u64 = 0;
    const X: u64 = 0;

    fun t(): u64 {
        use a::S::S;
        {
            use a::with_struct::X as S;
            S::foo(); // resolves to struct
            S // resolves to constant
        }
    }
}
