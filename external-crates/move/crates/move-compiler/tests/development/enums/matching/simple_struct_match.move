module 0x42::m {

    public struct Box<T> { x: T }

    fun test(b: Box<u8>) {
        match (b) {
            Box { x: 0 } => (),
            Box { x: _ } => (),
        }
    }

}
