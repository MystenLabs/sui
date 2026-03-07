//# publish
module 0x40::m {
    public fun next(n: u64): u64 {
        n + 1
    }
}

//# publish-and-call --call 0x42::m::a --call 0x40::m::next 1
module 0x42::m {
    use 0x40::m::next;

    fun a(): u64 {
        next(10)
    }
}
