// unused mut references are not unused if they come from other functions
module a::m {
    public fun foo(): &mut u64 {
        abort 0
    }
    public fun allowed() {
        foo();
    }
}
