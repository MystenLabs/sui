/*
/// Module: enums
module enums::enums;
*/

// For Move coding conventions, see
// https://docs.sui.io/concepts/sui-move-concepts/conventions


module enums::enums;

public enum Color has copy, drop {
    Red,
    Green,
    Blue
}

public fun is_red(color: Color): bool {
    match (color) {
        Color::Red => true,
        _ => false,
    }
}

public fun is_green(color: Color): bool {
    match (color) {
        Color::Green => true,
        _ => false,
    }
}

public fun is_blue(color: Color): bool {
    match (color) {
        Color::Blue => true,
        _ => false,
    }
}

