module basic::cast;

public fun cast_u64_to_u128(x: u64): u128 {
    (x as u128)
}

public fun cast_chain(x: u64): u256 {
    ((x as u128) as u256)
}

public fun cast_in_expr(x: u64, y: u64): u128 {
    (x as u128) + (y as u128)
}
