// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An escrow for atomic swap of objects that trusts a third party for liveness, but not safety.
module defi::escrow {
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// An object held in escrow
    struct EscrowedObj<T: key + store, phantom ExchangeForT: key + store> has key, store {
        id: UID,
        /// owner of the escrowed object
        sender: address,
        /// intended recipient of the escrowed object
        recipient: address,
        /// ID of the object `sender` wants in exchange
        // TODO: this is probably a bad idea if the object is mutable.
        // that can be fixed by asking for an additional approval
        // from `sender`, but let's keep it simple for now.
        exchange_for: ID,
        /// the escrowed object
        escrowed: T,
    }

    // Error codes
    /// The `sender` and `recipient` of the two escrowed objects do not match
    const EMismatchedSenderRecipient: u64 = 0;
    /// The `exchange_for` fields of the two escrowed objects do not match
    const EMismatchedExchangeObject: u64 = 1;

    /// Create an escrow for exchanging goods with
    /// `counterparty`, mediated by a `third_party`
    /// that is trusted for liveness
    public fun create<T: key + store, ExchangeForT: key + store>(
        recipient: address,
        third_party: address,
        exchange_for: ID,
        escrowed: T,
        ctx: &mut TxContext
    ) {
        let sender = tx_context::sender(ctx);
        let id = object::new(ctx);
        // escrow the object with the trusted third party
        transfer::public_transfer(
            EscrowedObj<T,ExchangeForT> {
                id, sender, recipient, exchange_for, escrowed
            },
            third_party
        );
    }

    /// Trusted third party can swap compatible objects
    public entry fun swap<T1: key + store, T2: key + store>(
        obj1: EscrowedObj<T1, T2>,
        obj2: EscrowedObj<T2, T1>,
    ) {
        let EscrowedObj {
            id: id1,
            sender: sender1,
            recipient: recipient1,
            exchange_for: exchange_for1,
            escrowed: escrowed1,
        } = obj1;
        let EscrowedObj {
            id: id2,
            sender: sender2,
            recipient: recipient2,
            exchange_for: exchange_for2,
            escrowed: escrowed2,
        } = obj2;
        object::delete(id1);
        object::delete(id2);
        // check sender/recipient compatibility
        assert!(&sender1 == &recipient2, EMismatchedSenderRecipient);
        assert!(&sender2 == &recipient1, EMismatchedSenderRecipient);
        // check object ID compatibility
        assert!(object::id(&escrowed1) == exchange_for2, EMismatchedExchangeObject);
        assert!(object::id(&escrowed2) == exchange_for1, EMismatchedExchangeObject);
        // everything matches. do the swap!
        transfer::public_transfer(escrowed1, sender2);
        transfer::public_transfer(escrowed2, sender1)
    }

    /// Trusted third party can always return an escrowed object to its original owner
    public entry fun return_to_sender<T: key + store, ExchangeForT: key + store>(
        obj: EscrowedObj<T, ExchangeForT>,
    ) {
        let EscrowedObj {
            id, sender, recipient: _, exchange_for: _, escrowed
        } = obj;
        object::delete(id);
        transfer::public_transfer(escrowed, sender)
    }
}
