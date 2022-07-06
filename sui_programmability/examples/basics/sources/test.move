module basics::test {
    use sui::id::VersionedID;
    use basics::counter::Counter;
    use sui::transfer;
    use sui::transfer::ChildRef;
    use sui::tx_context::TxContext;
    use sui::tx_context;

    struct Record has key {
        id: VersionedID,
        child_ref: ChildRef<Counter>,
    }

    public entry fun transfer(
        object: Counter,
        target: &mut Counter,
        ctx: &mut TxContext,
    ) {
        let child_ref = transfer::transfer_to_object(object, target);
        transfer::transfer(
            Record {
                id: tx_context::new_id(ctx),
                child_ref,
            },
            tx_context::sender(ctx),
        )
    }
}
