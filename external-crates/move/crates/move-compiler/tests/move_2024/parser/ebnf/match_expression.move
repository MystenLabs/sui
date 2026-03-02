// Test: Match expressions with various patterns
// EBNF: MatchExp, MatchArm, MatchPattern, AtPattern, ConstructorPattern
module 0x42::match_expression;

public enum Color has drop {
    Red,
    Green,
    Blue,
    Rgb(u8, u8, u8),
    Named { name: vector<u8> },
}

fun color_to_hex(c: Color): u64 {
    match (c) {
        Color::Red => 0xFF0000,
        Color::Green => 0x00FF00,
        Color::Blue => 0x0000FF,
        Color::Rgb(r, g, b) => (r as u64) << 16 | (g as u64) << 8 | (b as u64),
        Color::Named { .. } => 0x000000,
    }
}

fun with_guard(x: u64): bool {
    match (&x) {
        n if (*n < 10) => true,
        n if (*n >= 10 && *n < 100) => true,
        _ => false,
    }
}

fun with_or_pattern(c: Color): bool {
    match (c) {
        Color::Red | Color::Green | Color::Blue => true,
        Color::Rgb(..) | Color::Named { .. } => false,
    }
}

fun with_at_pattern(c: Color): Color {
    match (c) {
        x @ Color::Red => x,
        x @ _ => x,
    }
}

fun with_field_pattern(c: Color): u8 {
    match (c) {
        Color::Rgb(r, ..) => r,
        _ => 0,
    }
}
