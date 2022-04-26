// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module USDC::Abstract {
    use Std::Vector;
    use Sui::Balance::{Self, Balance};
    use Sui::Coin::{Self, TreasuryCap};
    use Sui::Transfer;
    use Sui::ID::{Self, VersionedID};
    use Sui::TxContext::{Self, TxContext};

    /// The Balance of a regulated Coin. Can be refilled only by
    /// applying a regulated `Transfer` using a verification `Registry`.
    struct OwnedBalance<phantom T> has key {
        id: VersionedID,
        /// A stored `Balance`; since it uses the same type as
        /// `Sui::Balance`, regulated coins are not reimplementing
        /// anything and rely on the existing system.
        balance: Balance<T>,
        /// Only owner can use his balance;
        /// Even though it is currently transferable, no one else
        /// should be able to use this Balance until it is moved
        /// to another owner.
        owner: address,
    }

    struct Transfer<phantom T> has key {
        id: VersionedID,
        balance: Balance<T>,
        receiver: address,
        sender: address,
    }

    /// This struct is going to hold addresses which have been
    /// banned from making transactions on the network. Technically
    /// we cannot forbid anyone to make transactions, but what we
    /// can do is make their funds unusable by restricting unwrapping.
    struct Registry<phantom T> has key {
        id: VersionedID,
        /// List of banned addresses which are not allowed to transfer/unwrap
        /// their coins.
        banned: vector<address>,
        /// Holds a TreasureCap of a Coin module, which opens the door
        /// for conversions between a regulated currency and non-regulated
        /// ones. Also it allows for using unified minting/burning security
        /// methods.
        treasury_cap: TreasuryCap<T>,
        /// For simplicity's sake use address auth for Registry operations.
        /// Further it can be extended to an AuthorityCap {} (possibly with
        /// voting system)
        owner: address,
    }

    /// Create a new regulated currency. To do so, first create a new
    /// currency through Coin module, and then share an object representing
    /// a regulated currency's Registry.
    public fun create_currency<T: drop>(
        witness: T,
        ctx: &mut TxContext
    ) {
        Transfer::share_object(Registry<T> {
            id: TxContext::new_id(ctx),
            banned: Vector::empty(),
            treasury_cap: Coin::create_currency<T>(witness, ctx),
            owner: TxContext::sender(ctx)
        })
    }

    /// Mint some amount of the regulated currency.
    /// To do so, use `Registry.treasury_cap`, mint `Sui::Coin`,
    /// and then turn it into the regulated Coin.
    public fun mint<T>(
        registry: &mut Registry<T>,
        value: u64,
        ctx: &mut TxContext
    ): OwnedBalance<T> {
        let owner = TxContext::sender(ctx);

        assert!(owner == registry.owner, 0); // ENOT_ALLOWED

        let treasury = &mut registry.treasury_cap;
        let generic_coin = Coin::mint(value, treasury, ctx);
        let balance = Coin::into_balance(generic_coin);

        OwnedBalance {
            owner,
            balance,
            id: TxContext::new_id(ctx),
        }
    }

    /// A protected transfer.
    /// Fails if one of the following conditions is not met:
    /// - Tx sender doesn't own OwnedBalance
    /// - Either sender or receiver are banned in the Registry
    public fun transfer<T>(
        registry: &Registry<T>,
        owned_balance: &mut OwnedBalance<T>,
        value: u64,
        receiver: address,
        ctx: &mut TxContext
    ) {
        let sender = TxContext::sender(ctx);

        assert!(Vector::contains(&registry.banned, &sender) == false, 1); // EADDRESS_BANNED
        assert!(Vector::contains(&registry.banned, &receiver) == false, 2); // EADDRESS_BANNED_REC
        assert!(owned_balance.owner == sender, 3); // ESTOLEN_BALANCE
        assert!(value <= Balance::value(&owned_balance.balance), 4); // ENOT_ENOUGH_FUNDS

        Transfer::transfer(Transfer<T> {
            sender,
            receiver,
            id: TxContext::new_id(ctx),
            balance: Balance::split(&mut owned_balance.balance, value),
        }, receiver);
    }

    /// Accept a transfer from another account.
    /// Fails if one of the following conditions is not met:
    /// - Transfer object was stolen
    /// - OwnedBalance is not owned by tx sender
    /// - Either receiver or sender of the tx is banned in the Registry
    public fun accept<T>(
        registry: &Registry<T>,
        owned_balance: &mut OwnedBalance<T>,
        transfer: Transfer<T>,
        ctx: &mut TxContext
    ) {
        let tx_sender = TxContext::sender(ctx);
        let Transfer { id, balance, receiver, sender } = transfer;

        assert!(Vector::contains(&registry.banned, &sender) == false, 1); // EADDRESS_BANNED
        assert!(Vector::contains(&registry.banned, &receiver) == false, 2); // EADDRESS_BANNED_REC
        assert!(owned_balance.owner == tx_sender, 3); // ESTOLEN_BALANCE
        assert!(receiver == tx_sender, 4); // ESOLEN_TRANSFER

        Balance::join(&mut owned_balance.balance, balance);
        ID::delete(id);
    }

    /// This method allows building on top of the regulated coin by
    /// authorizing borrows with a witness (which can only be created in
    /// the custom coin module).
    ///
    /// TODO: make a tutorial on Witness auth somewhere.
    public fun borrow_balance<T: drop>(
        _witness: T,
        owned_balance: &mut OwnedBalance<T>,
    ): &mut Balance<T> {
        &mut owned_balance.balance
    }
}

#[test_only]
module USDC::AbstractTests {
    use Sui::TestScenario::{Self, ctx};
    use USDC::Abstract;

    struct USDC has drop {}

    #[test]
    fun test_abstract() {
        let test = &mut TestScenario::begin(&@USDC);
        Abstract::create_currency(USDC {}, ctx(test));
    }
}
