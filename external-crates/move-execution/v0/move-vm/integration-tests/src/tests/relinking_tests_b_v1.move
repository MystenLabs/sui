/// Dependencies: [C v0+]
module 0x2::b {
    struct S { x: u64 }

    public fun b(): u64 {
        0x2::c::c() * 0x2::c::d()
    }
}
