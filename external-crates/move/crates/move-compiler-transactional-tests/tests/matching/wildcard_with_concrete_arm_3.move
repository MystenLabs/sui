//# init --edition 2024.beta

//# publish
module 0x42::t3 {

    public fun test(x: u64, b: bool): u64 {
        match (&x) {
            _ if (b) => 0,
            5 => 1,
            _ => 2,
        }
    }
}

//# run
module 0x43::main {
    use 0x42::t3;

    fun main() {
        assert!(t3::test(5, true) == 0, 0);
        assert!(t3::test(5, false) == 1, 1);
        assert!(t3::test(6, true) == 0, 2);
        assert!(t3::test(6, false) == 2, 3);
    }
}
