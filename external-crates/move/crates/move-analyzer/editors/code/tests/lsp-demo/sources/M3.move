module Symbols::M3 {

    /// Documented struct in another module
    struct OtherDocStruct has drop {
        some_field: u64,
    }

    /// Documented initializer in another module
    public fun create_other_struct(v: u64): OtherDocStruct {
        OtherDocStruct { some_field: v }
    }
}
