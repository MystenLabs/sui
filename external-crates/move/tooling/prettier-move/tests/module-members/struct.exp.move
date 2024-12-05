// options:
// useModuleLabel: true
// printWidth: 40

module prettier::struct;

use std::string::String;

// struct and abilities never break the line
public struct Default has key, drop, store {
    id: UID,
    field: String,
}

// we allow postfix ability declaration for
// structs as well
public struct Default2 {
    id: UID,
    field: String,
} has key, drop, store;

// abilities are always sorted in `key`,
// copy`, `drop`, `store` order
public struct Sort has key, copy, drop, store {}

// abilities support comments in between
public struct Cmt has /* please */ key /* don't */ , /* do */ store /* this */  {}

// line comments are also possible, but
// let's hope no one ever uses them
public struct Cmt2 has copy, drop, // kill me
store // trailing
 {}

// empty struct can be with comemtns
// both single and multi-line
public struct Cmt3 { /* block cmmt */ }
public struct Cmt4 {
    // line cmmt
}

// struct can be single line, and breaks
// automatically
public struct Struct has key { id: UID }

// broken into multiple lines
public struct Struct2 has key {
    id: UID,
    field: String,
}

// struct will first try to break on fields
// and then break on type arguments
public struct Struct3<phantom T> {
    field: String,
}

// struct can break on type arguments
public struct Struct4<
    phantom Token,
    Coin,
> { field: String }

// inner fields of the struct also break
// when possible - on type arguments
public struct Struct5<
    FirstParam,
    SecondParam,
> {
    field_name: ExtraLongType<
        FirstParam,
        SecondParam,
    >,
    another_field: String,
}

// === Positional ===

// positional struct breaks on fields and
// not on abilities
public struct Point(
    u64,
    u64,
) has key, drop, store;

// prefix abilities are also an option
public struct Point2 has key, drop, store (
    u64,
    u64,
)

// breaks on type arguments, but first
// tries on fields
public struct Container<phantom T>(
    u64,
    T,
)

// will break on type arguments, not fields
public struct Container2<
    phantom T,
    phantom C,
>(u64, T)

// will reformat into single line
public struct Point(u64, u64)

// allows line comments, they break the list
public struct Point(
    u64, // X
    u64, // Y
)

// allows block comments before types
/* what about this? */ public struct /* oh my god */ Point /* hello */ (
    /* X */ u64, /* post */
    /* Y */ u64, /* post okay */
    // trailing
) // trailing for Point

// allows block comments before types
public struct /* kill me */ Point(
    /* Leading Block */
    u64, // trailing
    /* Leading Block 2 */
    u64, // trailing
) // trailing
