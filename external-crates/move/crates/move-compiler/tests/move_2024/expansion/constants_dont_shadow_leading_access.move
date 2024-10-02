// module member aliases do not shadow leading access names
module a::S {
    public fun foo() {}
}

module a::with_module {
    const S: u64 = 0;
    const X: u64 = 0;

    fun t(): u64 {
        use a::S;
        S::foo(); // resolves to module
        S // resolves to constant
    }

    fun t2(): u64 {
        use a::S;
        {
            use a::with_module::X as S;
            S::foo(); // resolves to module
            S // resolves to constant
        }
    }
}
