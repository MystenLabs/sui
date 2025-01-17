module auth_events::capy;

use sui::event;
use auth_events::auth_events::{Self, StreamHead};
use std::type_name;

public struct Capy has key, store {
    id: UID,
    color: u8,
}

public struct CapyBorn has copy, drop {
    id: ID,
    color: u8,
}

fun init(ctx: &mut TxContext) {
    let stream_id = type_name::into_string(type_name::get<CapyBorn>());
    let stream_head = auth_events::create_stream(ctx, stream_id);
    transfer::public_share_object(stream_head);
}

public entry fun new(color: u8, stream_head: &mut StreamHead, ctx: &mut TxContext) {
    let capy = Capy {
        id: object::new(ctx),
        color: color,
    };
    emit_auth_event(CapyBorn {
        id: object::id(&capy),
        color: capy.color,
    }, stream_head);
    transfer::transfer(capy, tx_context::sender(ctx));
}

// TODO: Understand why using a generic is failing
public fun emit_auth_event(event: CapyBorn, stream_head: &mut StreamHead) {
    auth_events::add_to_stream(event, stream_head);
    event::emit(event);
}

// sui client ptb \
// --move-call "$PKG::capy::new" 3 \
// --gas-budget 10000000
