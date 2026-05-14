//# init --edition 2024.beta

//# publish
module 0x42::m {

    public fun is_zero(x: u64): bool {
        let 0u64 = x else { return false };
        true
    }

    public fun is_true(b: bool): u64 {
        let true = b else { return 0 };
        1
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{is_zero, is_true};

        assert!(is_zero(0) == true, 1);
        assert!(is_zero(1) == false, 2);
        assert!(is_zero(99) == false, 3);

        assert!(is_true(true) == 1, 4);
        assert!(is_true(false) == 0, 5);
    }
}
