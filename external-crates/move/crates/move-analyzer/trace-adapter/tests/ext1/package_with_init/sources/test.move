/// Module: test
module package_with_init::test;

use sui::coin;

public struct TEST has drop {}

fun init(witness: TEST, ctx: &mut TxContext) {
    let (treasury, metadata) = coin::create_currency(
        witness,
        6,
        b"TEST",
        b"",
        b"",
        option::none(),
        ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury, ctx.sender())
}
