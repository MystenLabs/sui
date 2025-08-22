module enums::directions;

public enum Direction has copy, drop {
    Up,
    Down,
    Left,
    Right
}

public fun is_up(direction: Direction): bool {
    match (direction) {
        Direction::Up => true,
        _ => false,
    }
}

public fun is_vertical(direction: Direction): bool {
    match (direction) {
        Direction::Up => true,
        Direction::Down => true,
        _ => false,
    }
}

public fun is_horizontal(direction: Direction): bool {
    match (direction) {
        Direction::Left | Direction::Right => true,
        _ => false,
    }
}
