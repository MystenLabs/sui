module a::m;

fun test(): bool {
    match (0u8) {
        255 => return true,
        0 => return false,
        _ => return false,
    }
}
