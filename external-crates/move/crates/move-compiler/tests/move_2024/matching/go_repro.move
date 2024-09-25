module 0x42::go;

public enum Colour has copy, store, drop {
    Empty,
    Black,
    White,
}

public fun from_index(index: u64): Colour {
    match (index) {
        0 => Colour::Empty,
        1 => Colour::Black,
        2 => Colour::White,
        _ => abort 0,
    }
}
