module 0x42::m {

    const Z: u64 = 0;
    const SZ: u64 = 1;

    public fun test(): u64 {
        let mut y: u64 = 1;
        match (y) {
            Z if ({y = y - 1; y == Z}) => 10,
            SZ if ({y = y - 1; y == Z}) => y,
            _n => 20,
        }
    }

}
