/// Dependencies: []
module 0x2::c {
    struct S { x: u64 }
    struct R { x: u64, y: u64 }
    struct Q { x: u64, y: u64, z: u64 }

    public fun c(): u64 {
        45
    }

    public fun d(): u64 {
        46
    }
}
