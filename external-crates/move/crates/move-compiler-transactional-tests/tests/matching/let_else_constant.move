//# init --edition 2024.beta

//# publish
module 0x42::m {

    const MAGIC: u64 = 42;

    public fun is_magic(x: u64): bool {
        let MAGIC = x else { return false };
        true
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::is_magic;

        assert!(is_magic(42) == true, 1);
        assert!(is_magic(0) == false, 2);
        assert!(is_magic(43) == false, 3);
    }
}
