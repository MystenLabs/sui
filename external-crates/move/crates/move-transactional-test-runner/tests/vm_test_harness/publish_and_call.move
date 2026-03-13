//# publish-and-call --call 0x42::m::a --call 0x42::m::b
module 0x42::m {
    fun a(): u64 {
        0
    }

    fun b(): u64 {
        0
    }
}
