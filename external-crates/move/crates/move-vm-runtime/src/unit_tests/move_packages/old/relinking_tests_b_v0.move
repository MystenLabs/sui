/// Dependencies: [C v0+]
module 0x7::b {
    public fun b(): u64 {
        0x7::c::c() + 1
    }
}
