//# init --edition 2024.beta

//# publish
module 0x42::m {

    public struct Pair has drop { x: u64, y: u64 }

    public fun make_pair(x: u64, y: u64): Pair { Pair { x, y } }

    public fun sum_pair(p: Pair): u64 {
        let Pair { x, y } = p else { return 0 };
        x + y
    }

    public struct Wrapper(u64) has drop;

    public fun make_wrapper(v: u64): Wrapper { Wrapper(v) }

    public fun unwrap(w: Wrapper): u64 {
        let Wrapper(v) = w else { return 0 };
        v
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{make_pair, sum_pair, make_wrapper, unwrap};

        assert!(sum_pair(make_pair(10, 20)) == 30, 1);
        assert!(unwrap(make_wrapper(99)) == 99, 2);
    }
}
