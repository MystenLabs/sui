// Test: Enum declarations with various variant types
// EBNF: EnumDecl, VariantDecl, VariantFields
module 0x42::enum_patterns;

public enum SimpleEnum has drop {
    Empty,
    Single(u64),
    Pair(u64, bool),
    Named { value: u64, flag: bool },
}

public enum Option<T> has copy, drop {
    None,
    Some(T),
}

public enum PhantomWrapper<phantom T> has drop {
    Marker,
    Data(u64),
}

public enum Result<T, E> has copy, drop, store {
    Ok(T),
    Err(E),
}

public enum Direction {
    North,
    South,
    East,
    West,
} has copy, drop;
