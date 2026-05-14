module 0x42::m {

    public struct Pair has drop { x: u64, y: u64 }

    fun named_fields(): u64 {
        let p = Pair { x: 1, y: 2 };
        let Pair { x, y } = p else { return 0 };
        x + y
    }

}
