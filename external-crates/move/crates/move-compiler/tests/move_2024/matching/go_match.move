module 0x42::go;

public enum Color has copy, store, drop {
    Empty,
    Black,
    White,
}

public fun from_index(color: Color): u64 {
    match (color) {
        Color::White => 0,
        Color::Black => 1,
        _ => abort 0,
    }
}
