//# init

//# publish
module 0x42::m {

    public enum AB<T> has drop {
        A(T, bool),
        B { x: T }
    }

    fun incr_container(x: AB<u64>): u64 {
        match (x) {
            AB::A(mut x, _) | AB::B { mut x } => {
                x = x + 1;
                x
            },
        }
    }

    public fun run() {
        let x = AB::A(0, true);
        let y = incr_container(x);
        assert!(y == 1, 0);
    }
}

//# run 0x42::m::run
