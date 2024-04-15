module Move2024::implicit_uses {

    public struct Obj {
        id: UID
    }

    public fun foo(ctx: &mut TxContext): Obj {
        Obj { id: object::new(ctx) }
    }
}
