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
