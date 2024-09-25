/// Dependencies: [B v0+, C v1+]
module 0x7::a {
    public fun a(): u64 {
        0x7::b::b() + 0x7::c::d()
    }
}
