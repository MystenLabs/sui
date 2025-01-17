/// Module: auth_events
module auth_events::auth_events;

use std::hash;
use std::type_name;
use sui::bcs;
use std::ascii::String;

/// Error code for when the user has no access.
const ENoAccess: u64 = 0;

// If [e1, e2, e3] are events, digest = H(H(e3), H(H(e2), H(H(e1)))), most_recent_event = H(e3)
public struct StreamHead has key, store {
    id: UID,
    stream_id: String,
    digest: vector<u8>, 
    most_recent_event_digest: vector<u8>,
    count: u64,
}

// Creates a stream given a stream identifier. 
public fun create_stream(ctx: &mut TxContext, stream_id: String): StreamHead {
    StreamHead {
        id: object::new(ctx),
        stream_id: stream_id,
        digest: vector::empty(),
        most_recent_event_digest: vector::empty(),
        count: 0,
    }
}

public fun hash_two(lhs: vector<u8>, rhs: vector<u8>): vector<u8> {
    let mut inputs = lhs;
    inputs.append(rhs);
    hash::sha3_256(inputs)
}

public fun add_to_stream<T: copy + drop>(
    event: T,
    stream_head: &mut StreamHead
) {
    // This check effectively acts as an access control because the events defined in a module
    // can only be instantiated by that module.
    // Note that we could support dynamic or runtime-defined streams by instead doing access control with a capability object.
    assert!(stream_head.stream_id == type_name::into_string(type_name::get<T>()), ENoAccess);

    stream_head.count = stream_head.count + 1;
    stream_head.most_recent_event_digest = hash::sha3_256(bcs::to_bytes(&event));
    stream_head.digest = hash_two(stream_head.most_recent_event_digest, stream_head.digest);
}

// ------------------------------------------------------------
// Testing functions

use std::ascii;

public struct TestEvent has copy, drop {
    color: u64,
}

#[test]
fun test_add_to_stream() {
    let mut ctx = tx_context::dummy();
    let type_name = type_name::into_string(type_name::get<TestEvent>());
    assert!(type_name == ascii::string(b"0000000000000000000000000000000000000000000000000000000000000000::auth_events::TestEvent"));

    let mut stream_head = create_stream(&mut ctx, type_name);
    assert!(stream_head.stream_id == type_name);
    assert!(stream_head.count == 0);
    assert!(stream_head.digest == vector::empty());
    assert!(stream_head.most_recent_event_digest == vector::empty());
    let mut current_digest = stream_head.digest;

    let mut num_events: u64 = 0;
    while (num_events < 100) {
        let test_event = TestEvent {
            color: num_events,
        };
        add_to_stream(test_event, &mut stream_head);
        num_events = num_events + 1;

        assert!(stream_head.count == num_events);
        assert!(stream_head.most_recent_event_digest == hash::sha3_256(bcs::to_bytes(&test_event)));
        current_digest = hash_two(stream_head.most_recent_event_digest, current_digest);
        assert!(stream_head.digest == current_digest);
    };

    transfer::public_share_object(stream_head);
}

#[test, expected_failure(abort_code = ENoAccess)]
fun test_add_to_stream_no_access() {
    let mut ctx = tx_context::dummy();
    let type_name = ascii::string(b"AnotherEvent");
    let mut stream_head = create_stream(&mut ctx, type_name);
    add_to_stream(TestEvent { color: 1 }, &mut stream_head);
    transfer::public_share_object(stream_head);
}