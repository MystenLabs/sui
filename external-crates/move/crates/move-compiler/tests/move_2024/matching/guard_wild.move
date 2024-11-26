module 0x0::repro;

public enum Tile has store, drop {
    Empty,
    Unwalkable,
}

public fun failure(tile: &Tile, value: bool): u64 {
    match (tile) {
        _ if (value) => 0,
        _ => 1,
    }
}
