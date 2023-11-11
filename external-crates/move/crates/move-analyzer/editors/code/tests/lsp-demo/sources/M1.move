module Symbols::M1 {

    const SOME_CONST: u64 = 42;

    struct SomeOtherStruct has drop {
        some_field: u64,
    }

    public fun some_other_struct(v: u64): SomeOtherStruct {
        SomeOtherStruct { some_field: v }
    }

    #[test]
    #[expected_failure]
    fun this_is_a_test() {
        1/0;
    }
}
