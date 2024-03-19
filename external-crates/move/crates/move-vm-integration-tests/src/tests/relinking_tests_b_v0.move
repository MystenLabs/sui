/// Dependencies: [C v0+]
module 0x2::b {
    public fun b(): u64 {
        0x2::c::c() + 1
    }
}
