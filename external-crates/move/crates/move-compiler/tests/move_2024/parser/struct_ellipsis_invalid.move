module 0x42::m {
    public struct X has drop {
        x: u64,
        y: bool,
        z: u64,
    }

    fun f(y: X): u64 {
        let .. = y;
        x
    }
}
