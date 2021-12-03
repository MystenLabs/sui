module 0x2::CreateTransferUse {
    use FastX::Authenticator::{Self, Authenticator};
    use FastX::ID::ID;
    use FastX::Transfer;
    use FastX::TxContext;

    struct S has key {
        id: ID,
        f: u64
    }

    /// Create an object and transfer it to `signer`
    public fun create(signer: signer, f: u64, inputs_hash: vector<u8>) {
        let ctx = TxContext::make_unsafe(signer, inputs_hash);
        let s = S { id: TxContext::new_id(&mut ctx), f };
        Transfer::transfer(s, TxContext::get_authenticator(&ctx));
    }

    fun transfer_(s: S, recipient: Authenticator) {
        Transfer::transfer(s, recipient)
    }

    public fun transfer(
        _signer: signer,
        id: address,
        recipient: address,
        _inputs_hash: vector<u8>
    ) acquires S {
        let s = move_from<S>(id);
        transfer_(s, Authenticator::new_from_address(recipient))
    }
}
