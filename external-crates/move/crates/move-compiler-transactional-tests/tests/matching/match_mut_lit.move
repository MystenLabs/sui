//# init --edition 2024.beta

//# publish
module 0x42::m {

    public fun t(x: &mut u64): u64 {
        match (x) {
            10 => 10,
            20 => 20,
            _ => 30,
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        let mut n = 10;
        assert!(m::t(&mut n) == 10);
        n = 20;
        assert!(m::t(&mut n) == 20);
        n = 21;
        assert!(m::t(&mut n) == 30);
    }
}
