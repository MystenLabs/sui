// options:
// printWidth: 30
// useModuleLabel: true

// Covers `enum_definition` node in grammar
module tests::enum;

use sui::vec_map::VecMap;

// empty enum example.
// gets fomatted but
// illegal in Move
public enum Empty {}

// enum variants will always
// new line, and end with a
// comma
public enum Test {
    // comments
    A, // are kept
    B,
}

// abilities do not break
public enum Test has store, copy, drop {
    A,
}

// trailing abilities are
// supported
public enum Test {
    A,
} has store, copy, drop;

// type parameters can break
public enum LongEnum<
    phantom A,
    phantom B,
> {
    A,
    B,
}

// positional, named and empty
// variants are supported
public enum Test<phantom C> { // comments don't break
    A,
    B { a: u8, b: u64 }, // trailing comments don't break
    C(u8, vector<u8>), // trailing comments don't break
}

// positional fields break
// same applies to named
public enum Test {
    VeryLongEnumVariantDoesntBreak,
    // breaks and keeps indent
    Positional(
        vector<u8>,
        VecMap<ID, u64>,
    ),
    // breaks and keeps indent
    NamedFields {
        field_one: vector<u8>, // trailing doesn't break the field
        field_two: u64,
    },
    // doesn't break on colon
    UltraLongNamed {
        long_field_decl: LongTypeName,
    }
}
