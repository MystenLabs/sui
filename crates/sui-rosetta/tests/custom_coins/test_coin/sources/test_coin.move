/// Module: test_coin
module test_coin::test_coin {

    use sui::coin;

    public struct TEST_COIN has drop {}

    fun init(witness: TEST_COIN, ctx: &mut TxContext) {
        let (treasury, metadata) = coin::create_currency(
            witness,
            6,
            b"TEST_COIN",
            b"",
            b"",
            option::none(),
            ctx
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, ctx.sender())
    }
}
