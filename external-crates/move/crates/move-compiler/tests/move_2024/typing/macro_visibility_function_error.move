module a::m {

    fun zero(): u64 { 0 }

    public macro fun test() {
    	zero()
    }
}

module a::n {
    use a::m::test;

    public fun t() {
        test!();
    }
}
