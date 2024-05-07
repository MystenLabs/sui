module 0x42::m {

    public fun t(): u64 {
        match (10 as u64) {
            x @ (y @ 10) => x + y,
            _ => 20
        }
    }

}
