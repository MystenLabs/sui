// Test: Positional struct and enum fields
// EBNF: StructFields, PosField, VariantFields
module 0x42::positional_fields;

public struct Pair(u64, u64) has copy, drop;

public struct Triple(u64, u64, u64) has copy, drop;

public struct Empty() has copy, drop;

public struct SingleField(bool) has copy, drop;

public struct WithAbilities(u64, bool) has copy, drop;

public enum PositionalEnum has drop {
    Unit,
    One(u64),
    Two(u64, bool),
    Three(u8, u16, u32),
}

fun access_positional(p: Pair): u64 {
    p.0 + p.1
}

fun create_pair(): Pair {
    Pair(10, 20)
}

fun match_positional(e: PositionalEnum): u64 {
    match (e) {
        PositionalEnum::Unit => 0,
        PositionalEnum::One(x) => x,
        PositionalEnum::Two(x, _) => x,
        PositionalEnum::Three(a, b, c) => (a as u64) + (b as u64) + (c as u64),
    }
}
