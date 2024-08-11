module 0x42::m {

    const Z: u64 = 0;

}

module 0x42::n {

    use 0x42::m;

    fun test(v: u64): u64 {
        match (v) {
            m::Z => 0,
            _ => v
        }
    }

}
