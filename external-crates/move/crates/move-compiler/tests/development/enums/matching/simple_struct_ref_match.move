module 0x42::m {

    public struct Box<T> { x: T }

    fun test(b: &Box<u8>): &u8 {
        match (b) {
            Box { x: y } => y,
        }
    }

}
