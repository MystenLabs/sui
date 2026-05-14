//# init --edition 2024.beta

//# publish
module 0x42::m {

    const MAGIC: u64 = 42;
    const MY_CONST: u64 = 99;

    public fun is_magic(x: u64): bool {
        let MAGIC = x else { return false };
        true
    }

    // Same theme, fully-qualified path: exercises the multi-segment
    // `name_access_chain_to_module_access` route for pattern constants.
    public fun is_my_const(x: u64): bool {
        let 0x42::m::MY_CONST = x else { return false };
        true
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{is_magic, is_my_const};

        assert!(is_magic(42) == true, 1);
        assert!(is_magic(0) == false, 2);
        assert!(is_magic(43) == false, 3);

        assert!(is_my_const(99) == true, 4);
        assert!(is_my_const(0) == false, 5);
        assert!(is_my_const(42) == false, 6);
    }
}
