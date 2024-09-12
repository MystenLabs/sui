//# init --edition 2024.beta

//# publish
module 0x42::m {

    public fun t(): u64 {
        match (10 as u64) {
            x @ (y @ 10) => x + y,
            _ => 20
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        assert!(m::t() == 20);
    }
}
