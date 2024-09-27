//# init --edition 2024.beta

//# publish
module 0x42::m {

    const Z: u64 = 0;
    const SZ: u64 = 1;

    public fun test(): u64 {
        let y: u64 = 1;
        match (y) {
            Z => 10,
            SZ if (SZ == 1) => 0,
            _n => 20,
        }
    }

}

//# run
module 0x43::main {

    fun main() {
        assert!(0x42::m::test() == 0, 1);
    }

}
