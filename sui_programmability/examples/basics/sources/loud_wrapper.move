// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example of data + Event wrapping. Allows attaching `Event` to a wrapper
/// so whenever an Wrapper is destroyed, a user-defined event is emitted
module basics::loud_wrapper {
    use sui::event::{Self, Event};
    use sui::id::{Self, VersionedID};
    use sui::tx_context::{new_id, TxContext};

    /// A generic wrapper for any type T; custom Event is attached to it
    /// to be emitted once the wrapper is destroyed.
    struct LoudWrapper<T: store, EVT: copy + drop + store> has key {
        id: VersionedID,
        t: T,
        evt: Event<EVT>
    }

    /// Wrap some T with an event EVT into a LoudWrapper.
    public fun wrap<T: store, EVT: copy + drop + store>(
        t: T, evt: EVT, ctx: &mut TxContext
    ): LoudWrapper<T, EVT> {
        LoudWrapper {
            id: new_id(ctx),
            t,
            evt: event::create(evt)
        }
    }

    /// Unwrap `LoudWrapper`: emit an Event<EVT> and return `T`.
    public fun unwrap<T: store, EVT: copy + drop + store>(wrapper: LoudWrapper<T, EVT>): T {
        let LoudWrapper { id, t, evt } = wrapper;

        id::delete(id);
        event::emit_event(evt);
        t
    }
}

#[test_only]
module basics::gift {
    use sui::id::{Self, ID};
    use sui::event;
    use sui::transfer;
    use sui::coin::{Coin};
    use sui::tx_context::TxContext;
    use sui::utf8::{Self, String};
    use basics::loud_wrapper::{Self as wrapper};

    /// Some message that can be read by the receiver.
    struct Gift<phantom T> has store {
        text: String,
        coin: Coin<T>
    }

    /// Emitted when a gift is sent. An ID is the ID of the Coin.
    struct GiftSent<phantom T> has copy, drop { id: ID }

    /// Emitted whenever the recipient opens the gift box and
    /// accesses the contents. ID here is the same as in the GiftSent
    /// event.
    struct GiftOpened<phantom T> has store, copy, drop { id: ID }

    /// Send a Gift as a generic LoudWrapper. Attach an event to it so
    /// we know when and who has received the package.
    public entry fun send<T>(
        coin: Coin<T>, text: vector<u8>, to: address, ctx: &mut TxContext
    ) {
        let coin_id = *id::id(&coin);
        let wrapper = wrapper::wrap(
            Gift { coin, text: utf8::string_unsafe(text) },
            GiftOpened<T> { id: *&coin_id },
            ctx
        );

        event::emit(GiftSent<T> { id: coin_id });
        transfer::transfer(wrapper, to)
    }
}

