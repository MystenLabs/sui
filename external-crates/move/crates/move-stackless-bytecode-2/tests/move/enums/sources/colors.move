// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module enums::colors;

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

