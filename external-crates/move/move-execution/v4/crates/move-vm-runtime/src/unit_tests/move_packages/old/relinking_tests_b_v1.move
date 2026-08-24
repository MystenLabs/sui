/// Dependencies: [C v0+]
module 0x7::b {
    public struct S { x: u64 }

    public fun b(): u64 {
        0x7::c::c() * 0x7::c::d()
    }
}
