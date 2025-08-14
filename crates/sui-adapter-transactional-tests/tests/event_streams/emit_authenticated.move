// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Exercise test functions that emit authenticated events

//# init --addresses test=0x0 --accounts A B --simulator

//# publish

module test::event_streams {
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

    public entry fun delete(o: Object) {
        let Object { id, event_stream, event_stream_cap } = o;
        event_stream_cap.destroy();
        event_stream.destroy();
        object::delete(id);
    }
}

//# run test::event_streams::create --sender A --args @A

//# view-object 2,0

//# run test::event_streams::emit_event --sender A --args object(2,0) 10

//# create-checkpoint

//# view-object 4,0

// Checkpoint 2: Add second event - should trigger MMR merge at height 1
//# run test::event_streams::emit_event --sender A --args object(2,0) 20

//# create-checkpoint

//# view-object 4,0

// Checkpoint 3: Add third event - should place at MMR[0] 
//# run test::event_streams::emit_event --sender A --args object(2,0) 30

//# create-checkpoint

//# view-object 4,0

// Checkpoint 4: Add fourth event - should trigger cascade merge to MMR[2]
//# run test::event_streams::emit_event --sender A --args object(2,0) 40

//# create-checkpoint

//# view-object 4,0

//# run test::event_streams::delete --sender A --args object(2,0)



// Run the test with:
// $ cargo nextest run -p sui-adapter-transactional-tests emit_authenticated