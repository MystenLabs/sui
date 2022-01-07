module 0x2::CreateTransferUse {
    use FastX::Address::{Self, Address};
    use FastX::ID::ID;
    use FastX::Transfer;
    use FastX::TxContext::{Self, TxContext};

    struct S has key {
        id: ID,
        f: u64
    }

    /// Create an object and transfer it to `recipient`
    public fun create(f: u64, recipient: vector<u8>, ctx: TxContext) {
        Transfer::transfer(
            S { id: TxContext::new_id(&mut ctx), f },
            Address::new(recipient)
        )
    }

    public fun transfer(s: S, recipient: Address, _ctx: TxContext) {
        Transfer::transfer(s, recipient)
    }

    public fun use_it(s: &S, _ctx: TxContext) {
        s.f;
    }
}
