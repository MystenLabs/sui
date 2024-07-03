module 0x42::m {

    public struct Cup<T>(T)

    fun main(x: u64): bool {
        let _x = x as Cup<u8>;
        x as u64 < 0
    }
}
