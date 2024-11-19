module 0x42::M {
    spec_block
    fun with_emits<T: drop>(_guid: vector<u8>, _msg: T, x: u64): u64 {
        x
    }
}
