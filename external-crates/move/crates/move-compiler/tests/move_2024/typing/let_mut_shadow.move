// double check mut respects shadowing
module a::m {
    public fun foo(x: u64) {
        {
            let mut x = 0;
            x = x + 1;
            bar(x)
        };
        x = x + 1;
        x;
    }
    fun bar(_: u64) {}
}
