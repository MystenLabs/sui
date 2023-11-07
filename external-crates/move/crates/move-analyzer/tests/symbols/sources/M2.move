module Symbols::M2 {

    struct SomeOtherStruct has drop {
        some_field: u64,
    }

    public fun some_other_struct(v: u64): SomeOtherStruct {
        SomeOtherStruct { some_field: v }
    }

    public fun multi_arg(p1: u64, p2: u64): u64 {
        p1 + p2
    }

}
