module 0x42::m {

    fun t(): u64 {
        match (10 as u64) {
            x @ 5 => x,
            _ => 10
        }
    }

}
