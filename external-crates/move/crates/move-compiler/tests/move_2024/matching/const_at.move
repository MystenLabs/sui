//# init

//# publish
module 0x42::m {

    const Z: u64 = 0;
    const SZ: u64 = 1;

    public fun test(): u64 {
        let y: u64 = 1;
        match (y) {
            Z => 10,
            x @ SZ => x,
            _n => 20,
        }
    }

}

//# run
module 0x42::main {

    fun main() {
        assert!(0x42::m::test() == 1, 1);
    }

}
