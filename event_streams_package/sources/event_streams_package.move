// module event_streams_package::event_streams_package;

// For Move coding conventions, see
// https://docs.sui.io/concepts/sui-move-concepts/conventions


module event_streams_package::event_streams_package {
    use sui::event;

    public struct Object has key, store {
        id: UID,
        event_stream: event::EventStream,
        event_stream_cap: event::EventStreamCap,
    }

    public struct AuthEvent has copy, drop {
        value: u64
    }

    public entry fun create(recipient: address, ctx: &mut TxContext) {
        let stream = event::new_event_stream(ctx);
        let cap = stream.get_cap(ctx);
        transfer::public_transfer(
            Object {
                id: object::new(ctx),
                event_stream: stream,
                event_stream_cap: cap,
            },
            recipient
        )
    }

    // test that reading o2 and updating o1 works
    public entry fun emit_event(o: &mut Object, value: u64) {
        // emit an event so the world can see the new value
        let ev = AuthEvent { value };
        o.event_stream_cap.emit(ev);
    }

    public entry fun emit_normal_event(o: &mut Object, value: u64) {
        let ev = AuthEvent { value };
        event::emit(ev);
    }

    public entry fun delete(o: Object) {
        let Object { id, event_stream, event_stream_cap } = o;
        event_stream_cap.destroy();
        event_stream.destroy();
        object::delete(id);
    }
}
