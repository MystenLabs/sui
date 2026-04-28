module 0x42::m {

    public struct Wrapper(u64) has drop;

    fun positional(): u64 {
        let w = Wrapper(99);
        let Wrapper(v) = w else { return 0 };
        v
    }

}
