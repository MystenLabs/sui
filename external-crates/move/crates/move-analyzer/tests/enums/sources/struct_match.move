module Enums::struct_match {

    public struct SomeStruct has drop {
        some_field: u64,
    }

    public struct AnotherStruct has drop {
        field: u64,
        another_field: SomeStruct,
    }

    public fun struct_match(s: AnotherStruct): u64 {
        match (s) {
            AnotherStruct { field, .. } => field,
            AnotherStruct { .. , another_field: SomeStruct { some_field } } => some_field,
            _ => 42,
        }
    }
}
