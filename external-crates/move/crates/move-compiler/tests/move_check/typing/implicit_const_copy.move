// implicitly copy constants result in a warning
module a::m {
    const C: u64 = 0;
    const BYTES: vector<u8> = b"hello";

    fun check() {
        &C;
        &BYTES;
        *&C;
        *&BYTES;
        &mut C;
        &mut BYTES;
        *&mut C = 1;
        *&mut BYTES = b"bye";
    }
}
