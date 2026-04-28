module 0x42::m {

    const MY_CONST: u64 = 42;

    fun match_constant(x: u64): u64 {
        let MY_CONST = x else { return 0 };
        1
    }

}
