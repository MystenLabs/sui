module paywalrus::other;

public struct Box<T> has key {
    id: UID,
    value: T,
}

public fun create_box<T>(value: T, ctx: &mut TxContext): Box<T> {
    Box {
        id: object::new(ctx),
        value,
    }
}

public fun box_id<T>(box: &Box<T>): ID {
    box.id.to_inner()
}

/// Maximum value for a `u64`
public macro fun max_value(): u64 {
    0x0000_0000_FFFF_FFFF
}

public fun bitwise_not(x: u64): u64 {
    x ^ max_value!()
}
