/// Dependencies: [B v0+, C v1+]
module 0x4::a {
    public fun a(): u64 {
        0x3::b::b() + 0x2::c::d()
    }
}
