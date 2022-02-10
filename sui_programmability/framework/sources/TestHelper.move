#[test_only]
module FastX::TestHelper {
    use FastX::Address;
    use FastX::TxContext::{Self, TxContext};

    /// Returns the object last received (through transfer) by signer of `ctx`.
    /// It can be from either a normal transfer or freeze_after_transfer.
    /// This function can only be used in testing because any change to the object
    /// could result in inconsistency with Sui storage in a prod system.
    public fun get_last_received_object<T: key>(ctx: &TxContext): T {
        get_last_received_object_internal(Address::into_bytes(TxContext::get_signer_address(ctx)))
    }

    native fun get_last_received_object_internal<T: key>(signer_address: vector<u8>): T;

    // TODO: Add more APIs
}