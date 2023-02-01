// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A simple counter Application that follows simple rules:
/// - anyone can increment the counter
/// - only admin can reset the counter
///
/// Used as an illustration for the Axelar GMP protocol.
module axelar::counter {
    use sui::transfer;
    use sui::object::{Self, ID, UID};
    use sui::tx_context::{TxContext, sender};

    /// For when someone is trying to reset the counter with a
    /// wrong CounterOwnerCap.
    const ENotOwner: u64 = 0;

    /// A shared counter.
    struct Counter has key {
        id: UID,
        value: u64
    }

    /// A Capability given to the owner of a counter.
    /// Locks to a specific Counter, allows resetting the value.
    struct CounterOwnerCap has key, store {
        id: UID,
        counter: ID
    }

    /// Get the value of the Counter.
    public fun value(counter: &Counter): u64 {
        counter.value
    }

    /// Create and share a Counter object.
    /// Also give creator a CounterOwnerCap.
    public entry fun create(ctx: &mut TxContext) {
        let uid = object::new(ctx);

        transfer::transfer(CounterOwnerCap {
            id: object::new(ctx),
            counter: object::uid_to_inner(&uid)
        }, sender(ctx));

        transfer::share_object(Counter {
            id: uid,
            value: 0
        })
    }

    /// Increment a counter by 1.
    /// Can be performed by any account on the network.
    public entry fun increment(counter: &mut Counter) {
        counter.value = counter.value + 1;
    }

    /// Reset the counter. Can only be perfomed by Counter Owner.
    /// In this example, the only way to reset a counter is to receive a message
    /// from Axelar GMP.
    public entry fun reset(cap: &CounterOwnerCap, counter: &mut Counter) {
        assert!(object::uid_to_inner(&counter.id) == cap.counter, ENotOwner)
    }
}

/// Implementation of the GMP interface which receives messages from
/// the Axelar network and resets the counter on delivery.
///
/// Generic, can be initialized by any Counter Owner for their Counter objects.
module axelar::counter_gmp {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};

    use axelar::messenger::{Self, Axelar, Channel};
    use axelar::counter::{Self, Counter, CounterOwnerCap};

    /// A shared object that holds the CounterOwnerCap inside.
    /// Can be created by anyone for any Counter as long as the
    /// account is the creator of a Counter.
    struct CounterGate has key {
        id: UID,
        channel: Channel<CounterOwnerCap>
    }

    /// Create and share a new CounterGate object.
    entry public fun create(cap: CounterOwnerCap, ctx: &mut TxContext) {
        sui::transfer::share_object(CounterGate {
            id: object::new(ctx),
            channel: messenger::create_channel(cap, ctx)
        })
    }

    /// Reset the counter when message is delivered to this channel.
    /// We don't check the contents of the message as long as it is correctly
    /// consumed by the application.
    ///
    /// Operation fails if:
    /// - the message is delivered to a wrong channel
    /// - the message was already processed by this channel
    /// - CounterGate holds a Cap for another Counter
    entry public fun reset(
        counter: &mut Counter,
        gate: &mut CounterGate,
        axelar: &mut Axelar,
        msg_id: vector<u8>
    ) {
        let (cap_ref, _, _, _, _) = messenger::consume_message(axelar, &mut gate.channel, msg_id);

        counter::reset(cap_ref, counter);
    }
}
