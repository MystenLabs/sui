address 0x42 {
module M {
    fun t(cond: bool): u64 {
        loop {
            if (cond) break;
            return 0
        };
        0
    }
}
}
