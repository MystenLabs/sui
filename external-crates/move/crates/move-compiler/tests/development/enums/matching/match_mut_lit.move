module 0x42::m {

    public fun t(x: &mut u64): u64 {
        match (x) {
            10 => 10,
            20 => 20,
            _ => 30,
        }
    }

}
