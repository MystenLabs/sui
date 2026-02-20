module 0x2::M {
    fun f(): u64 {
        match (abort 0u64) {
            0u64 => 0u64,
            _ => 0u64,
        }
    }
}
