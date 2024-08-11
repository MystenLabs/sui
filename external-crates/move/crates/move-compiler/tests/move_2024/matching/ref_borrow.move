module 0x42::m {

    public struct Box<T> { x: T }

    fun t(opt: &Box<u64>): &u64 {
        let Box { x: z } = opt;
        z
    }

}
