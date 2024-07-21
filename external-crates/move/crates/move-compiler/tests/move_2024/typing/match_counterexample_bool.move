module 0x42::m {

    fun t(): u64 {
        let x = true;
        match (x) {
            true => 10
        }
    }

}
