module 0x2a::M {
    fun f(): u64 {
        let _v = vector[{ abort 0 }];
        0
    }
}
