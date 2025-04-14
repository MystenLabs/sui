module specs::tx_context_spec;

use sui::tx_context::{derive_id, fresh_object_address, TxContext, sender, digest, epoch, epoch_timestamp_ms};
use prover::prover::{ensures, old};

#[spec(target = sui::tx_context::fresh_object_address)]
fun fresh_object_address_spec(ctx: &mut TxContext): address {
    let old_ctx = old!(ctx);
    let result = fresh_object_address(ctx);
    ensures(sender(ctx) == sender(old_ctx));
    ensures(digest(ctx) == digest(old_ctx));
    ensures(epoch(ctx) == epoch(old_ctx));
    ensures(epoch_timestamp_ms(ctx) == epoch_timestamp_ms(old_ctx));
    result
}

#[spec(target = sui::tx_context::derive_id)]
fun derive_id_spec(tx_hash: vector<u8>, ids_created: u64): address {
    derive_id(tx_hash, ids_created)
}