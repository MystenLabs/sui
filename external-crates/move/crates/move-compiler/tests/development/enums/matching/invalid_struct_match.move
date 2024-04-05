module 0x42::m {

    public struct Box<T> { x: T }

    fun test(opt: &Box<u8>) {
        match (opt) {
            Box { x: T } => (),
        }
    }

}
