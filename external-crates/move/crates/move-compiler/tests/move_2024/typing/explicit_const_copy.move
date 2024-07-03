// implicitly copy constants result in a warning
module a::m {
    const C: u64 = 0;
    const BYTES: vector<u8> = b"hello";

    fun check() {
        copy C;
        copy BYTES;
    }
}
