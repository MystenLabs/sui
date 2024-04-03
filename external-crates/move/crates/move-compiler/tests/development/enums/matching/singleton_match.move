module 0x42::m {

    public enum Empty {
        None
    }

    fun main(e: &Empty) {
        match (e) {
            Empty::None => ()
        }
    }

}
