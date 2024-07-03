module 0x42::m {

    const TRUE: bool = true;
    const FALSE: bool = false;

    fun test(b: bool): u64 {
        match (b) {
            TRUE => 1,
            FALSE => 0,
            _ => 100,
        }
    }
}
