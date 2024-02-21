module 0x42::m {

    public enum Empty has drop {
        None
    }

    fun main() {
        let _x = Empty::None { };
        let _x = Empty::None();
    }

}
