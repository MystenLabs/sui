//# publish-and-call --call 0x42::m::a 0 --call 0x42::m::b vector[1,2,3]
module 0x42::m {
    fun a(n: u64): u64 {
        n + 1
    }

    fun b(_y: vector<u64>): u64 {
        10
    }
}
