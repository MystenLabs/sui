// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An escrow for atomic swap of objects without a trusted third party
module DeFi::SharedEscrow {
    use Std::Option::{Self, Option};

    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// An object held in escrow
    struct EscrowedObj<T: key + store, phantom ExchangeForT: key + store> has key, store {
        id: VersionedID,
        /// owner of the escrowed object
        creator: address,
        /// intended recipient of the escrowed object
        recipient: address,
        /// ID of the object `creator` wants in exchange
        exchange_for: ID,
        /// the escrowed object
        escrowed: Option<T>,
    }

    // Error codes
    /// An attempt to cancel escrow by a different user than the owner
    const EWrongOwner: u64 = 0;
    /// Exchange by a different user than the `recipient` of the escrowed object
    const EWrongRecipient: u64 = 1;
    /// Exchange with a different item than the `exchange_for` field
    const EWrongExchangeObject: u64 = 2;
    /// The escrow has already been exchanged or cancelled
    const EAlreadyExchangedOrCancelled: u64 = 3;

    /// Create an escrow for exchanging goods with counterparty
    public fun create<T: key + store, ExchangeForT: key + store>(
        recipient: address,
        exchange_for: ID,
        escrowed_item: T,
        ctx: &mut TxContext
    ) {
        let creator = TxContext::sender(ctx);
        let id = TxContext::new_id(ctx);
        let escrowed = Option::some(escrowed_item);
        Transfer::share_object(
            EscrowedObj<T,ExchangeForT> {
                id, creator, recipient, exchange_for, escrowed
            }
        );
    }

    /// The `recipient` of the escrow can exchange `obj` with the escrowed item
    public(script) fun exchange<T: key + store, ExchangeForT: key + store>(
        obj: ExchangeForT,
        escrow: &mut EscrowedObj<T, ExchangeForT>,
        ctx: &mut TxContext
    ) {
        assert!(Option::is_some(&escrow.escrowed), EAlreadyExchangedOrCancelled);
        let escrowed_item = Option::extract<T>(&mut escrow.escrowed);
        assert!(&TxContext::sender(ctx) == &escrow.recipient, EWrongRecipient);
        assert!(ID::id(&obj) == &escrow.exchange_for, EWrongExchangeObject);
        // everything matches. do the swap!
        Transfer::transfer(escrowed_item, TxContext::sender(ctx));
        Transfer::transfer(obj, escrow.creator);
    }

    /// The `creator` can cancel the escrow and get back the escrowed item
    public(script) fun cancel<T: key + store, ExchangeForT: key + store>(
        escrow: &mut EscrowedObj<T, ExchangeForT>,
        ctx: &mut TxContext
    ) {
        assert!(&TxContext::sender(ctx) == &escrow.creator, EWrongOwner);
        assert!(Option::is_some(&escrow.escrowed), EAlreadyExchangedOrCancelled);
        Transfer::transfer(Option::extract<T>(&mut escrow.escrowed), escrow.creator);
    }
}
