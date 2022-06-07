// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module representing a common type for regulated coins. Features balance
/// accessors which can be used to implement a RegulatedCoin interface.
///
/// To implement any of the methods, module defining the type for the currency
/// is expected to implement the main set of methods such as `borrow()`,
/// `borrow_mut()` and `zero()`.
///
/// Each of the methods of this module requires a Witness struct to be sent.
module RC::RegulatedCoin {
    use Sui::Balance::{Self, Balance};
    use Sui::TxContext::{Self, TxContext};
    use Sui::ID::VersionedID;

    /// The RegulatedCoin struct; holds a common `Balance<T>` which is compatible
    /// with all the other Coins and methods, as well as the `creator` field, which
    /// can be used for additional security/regulation implementations.
    struct RegulatedCoin<phantom T> has key {
        id: VersionedID,
        balance: Balance<T>,
        creator: address
    }

    /// Get the `RegulatedCoin.balance.value` field;
    public fun value<T>(c: &RegulatedCoin<T>): u64 {
        Balance::value(&c.balance)
    }

    /// Get the `RegulatedCoin.creator` field;
    public fun creator<T>(c: &RegulatedCoin<T>): address {
        c.creator
    }

    // === Necessary set of Methods (provide security guarantees and balance access) ===

    /// Get an immutable reference to the Balance of a RegulatedCoin;
    public fun borrow<T: drop>(_: T, coin: &RegulatedCoin<T>): &Balance<T> {
        &coin.balance
    }

    /// Get a mutable reference to the Balance of a RegulatedCoin;
    public fun borrow_mut<T: drop>(_: T, coin: &mut RegulatedCoin<T>): &mut Balance<T> {
        &mut coin.balance
    }

    /// Author of the currency can restrict who is allowed to create new balances;
    public fun zero<T: drop>(_: T, creator: address, ctx: &mut TxContext): RegulatedCoin<T> {
        RegulatedCoin { id: TxContext::new_id(ctx), balance: Balance::zero(), creator }
    }

    /// Build a transferable `RegulatedCoin` from a `Balance`;
    public fun from_balance<T: drop>(
        _: T, balance: Balance<T>, creator: address, ctx: &mut TxContext
    ): RegulatedCoin<T> {
        RegulatedCoin { id: TxContext::new_id(ctx), balance, creator }
    }

    /// Destroy `RegulatedCoin` and return its `Balance`;
    public fun into_balance<T: drop>(_: T, coin: RegulatedCoin<T>): Balance<T> {
        let RegulatedCoin { balance, creator: _, id } = coin;
        Sui::ID::delete(id);
        balance
    }

    // === Optional Methods (can be used for simpler implementation of basic operations) ===

    /// Join Balances of a `RegulatedCoin` c1 and `RegulatedCoin` c2.
    public fun join<T: drop>(witness: T, c1: &mut RegulatedCoin<T>, c2: RegulatedCoin<T>) {
        Balance::join(&mut c1.balance, into_balance(witness, c2))
    }

    /// Subtract `RegulatedCoin` with `value` from `RegulatedCoin`.
    ///
    /// This method does not provide any checks by default and can possibly lead to mocking
    /// behavior of `RegulatedCoin::zero()` when a value is 0. So in case empty balances
    /// should not be allowed, this method should be additionally protected against zero value.
    public fun split<T: drop>(
        witness: T, c1: &mut RegulatedCoin<T>, creator: address, value: u64, ctx: &mut TxContext
    ): RegulatedCoin<T> {
        let balance = Balance::split(&mut c1.balance, value);
        from_balance(witness, balance, creator, ctx)
    }
}

/// ABC is a RegulatedCoin which:
///
/// - is managed account creation (only admins can create a new balance)
/// - has a denylist for addresses managed by the coin admins
/// - has restricted transfers which can not be taken by anyone except the recipient
module ABC::ABC {
    use RC::RegulatedCoin::{Self as RCoin, RegulatedCoin as RCoin};
    use Sui::TxContext::{Self, TxContext};
    use Sui::Balance::{Self, Balance};
    use Sui::Coin::{Self, Coin, TreasuryCap};
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer;
    use Std::Vector;

    /// The ticker of ABC regulated token
    struct ABC has drop {}

    /// A restricted transfer of ABC to another account.
    struct Transfer has key {
        id: VersionedID,
        balance: Balance<ABC>,
        to: address,
    }

    /// A registry of addresses banned from using the coin.
    struct Registry has key {
        id: VersionedID,
        banned: vector<address>,
        swapped_amount: u64,
    }

    /// For when an attempting to interact with another account's RegulatedCoin<ABC>.
    const ENotOwner: u64 = 1;

    /// For when address has been banned and someone is trying to access the balance
    const EAddressBanned: u64 = 2;

    /// Create the ABC currency and send the TreasuryCap to the creator
    /// as well as the first (and empty) balance of the RegulatedCoin<ABC>.
    ///
    /// Also creates a shared Registry which holds banned addresses.
    fun init(ctx: &mut TxContext) {
        let treasury_cap = Coin::create_currency(ABC {}, ctx);
        let sender = TxContext::sender(ctx);

        Transfer::transfer(zero(sender, ctx), sender);
        Transfer::transfer(treasury_cap, sender);

        Transfer::share_object(Registry {
            id: TxContext::new_id(ctx),
            banned: Vector::empty(),
            swapped_amount: 0,
        });
    }

    // === Getters section: Registry ===

    /// Get total amount of `Coin` from the `Registry`.
    public fun swapped_amount(r: &Registry): u64 {
        r.swapped_amount
    }

    /// Get vector of banned addresses from `Registry`.
    public fun banned(r: &Registry): &vector<address> {
        &r.banned
    }

    // === Admin actions: creating balances, minting coins and banning addresses ===

    /// Create an empty `RCoin<ABC>` instance for account `for`. TreasuryCap is passed for
    /// authentification purposes - only admin can create new accounts.
    public(script) fun create(_: &TreasuryCap<ABC>, for: address, ctx: &mut TxContext) {
        Transfer::transfer(zero(for, ctx), for)
    }

    /// Mint more ABC. Requires TreasuryCap for authorization, so can only be done by admins.
    public(script) fun mint(treasury: &mut TreasuryCap<ABC>, owned: &mut RCoin<ABC>, value: u64, _: &mut TxContext) {
        Balance::join(borrow_mut(owned), Coin::mint_balance(value, treasury))
    }

    /// Burn `value` amount of `RCoin<ABC>`. Requires TreasuryCap for authorization, so can only be done by admins.
    ///
    /// TODO: Make TreasuryCap a part of Balance module instead of Coin.
    public(script) fun burn(treasury: &mut TreasuryCap<ABC>, owned: &mut RCoin<ABC>, value: u64, ctx: &mut TxContext) {
        Coin::burn(Coin::withdraw(borrow_mut(owned), value, ctx), treasury);
    }

    /// Ban some address and forbid making any transactions from or to this address.
    /// Only owner of the TreasuryCap can perform this action.
    public(script) fun ban(_cap: &TreasuryCap<ABC>, registry: &mut Registry, to_ban: address, _: &mut TxContext) {
        Vector::push_back(&mut registry.banned, to_ban)
    }

    // === Public: Regulated transfers ===

    /// Transfer entrypoint - create a restricted `Transfer` instance and transfer it to the
    /// `to` account for being accepted later.
    /// Fails if sender is not an creator of the `RegulatedCoin` or if any of the parties is in
    /// the ban list in Registry.
    public(script) fun transfer(r: &Registry, coin: &mut RCoin<ABC>, value: u64, to: address, ctx: &mut TxContext) {
        let sender = TxContext::sender(ctx);

        assert!(RCoin::creator(coin) == sender, ENotOwner);
        assert!(Vector::contains(&r.banned, &to) == false, EAddressBanned);
        assert!(Vector::contains(&r.banned, &sender) == false, EAddressBanned);

        Transfer::transfer(Transfer {
            to,
            id: TxContext::new_id(ctx),
            balance: Balance::split(borrow_mut(coin), value),
        }, to)
    }

    /// Accept an incoming transfer by joining an incoming balance with an owned one.
    ///
    /// Fails if:
    /// 1. the `RegulatedCoin<ABC>.creator` does not match `Transfer.to`;
    /// 2. the address of the creator/recipient is banned;
    public(script) fun accept_transfer(r: &Registry, coin: &mut RCoin<ABC>, transfer: Transfer, _: &mut TxContext) {
        let Transfer { id, balance, to } = transfer;

        assert!(RCoin::creator(coin) == to, ENotOwner);
        assert!(Vector::contains(&r.banned, &to) == false, EAddressBanned);

        Balance::join(borrow_mut(coin), balance);
        ID::delete(id)
    }

    // === Public: Swap RegulatedCoin <-> Coin ===

    /// Take `value` amount of `RegulatedCoin` and make it freely transferable by wrapping it into
    /// a `Coin`. Update `Registry` to keep track of the swapped amount.
    ///
    /// Fails if:
    /// 1. `RegulatedCoin<ABC>.creator` was banned;
    /// 2. `RegulatedCoin<ABC>` is not owned by the tx sender;
    public(script) fun take(r: &mut Registry, coin: &mut RCoin<ABC>, value: u64, ctx: &mut TxContext) {
        let sender = TxContext::sender(ctx);

        assert!(RCoin::creator(coin) == sender, ENotOwner);
        assert!(Vector::contains(&r.banned, &sender) == false, EAddressBanned);

        // Update swapped amount for Registry to keep track of non-regulated amounts.
        r.swapped_amount = r.swapped_amount + value;

        Transfer::transfer(Coin::withdraw(borrow_mut(coin), value, ctx), TxContext::sender(ctx));
    }

    /// Take `Coin` and put to the `RegulatedCoin`'s balance.
    ///
    /// Fails if:
    /// 1. `RegulatedCoin<ABC>.creator` was banned;
    /// 2. `RegulatedCoin<ABC>` is not owned by the tx sender;
    public(script) fun put_back(r: &mut Registry, rc_coin: &mut RCoin<ABC>, coin: Coin<ABC>, ctx: &mut TxContext) {
        let balance = Coin::into_balance(coin);
        let sender = TxContext::sender(ctx);

        assert!(RCoin::creator(rc_coin) == sender, ENotOwner);
        assert!(Vector::contains(&r.banned, &sender) == false, EAddressBanned);

        // Update swapped amount as in `swap_regulated`.
        r.swapped_amount = r.swapped_amount - Balance::value(&balance);

        Balance::join(borrow_mut(rc_coin), balance);
    }

    // === Private implementations accessors and type morphing ===

    fun borrow(coin: &RCoin<ABC>): &Balance<ABC> { RCoin::borrow(ABC {}, coin) }
    fun borrow_mut(coin: &mut RCoin<ABC>): &mut Balance<ABC> { RCoin::borrow_mut(ABC {}, coin) }
    fun zero(creator: address, ctx: &mut TxContext): RCoin<ABC> { RCoin::zero(ABC {}, creator, ctx) }

    fun into_balance(coin: RCoin<ABC>): Balance<ABC> { RCoin::into_balance(ABC {}, coin) }
    fun from_balance(balance: Balance<ABC>, creator: address, ctx: &mut TxContext): RCoin<ABC> {
        RCoin::from_balance(ABC {}, balance, creator, ctx)
    }

    // === Testing utilities ===

    #[test_only] public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }
    #[test_only] public fun borrow_for_testing(coin: &RCoin<ABC>): &Balance<ABC> { borrow(coin) }
    #[test_only] public fun borrow_mut_for_testing(coin: &mut RCoin<ABC>): &Balance<ABC> { borrow_mut(coin) }
}

#[test_only]
/// Tests for the ABC module. They are sequential and based on top of each other.
/// ```
/// * - test_minting
/// |   +-- test_creation
/// |       +-- test_transfer
/// |           +-- test_burn
/// |           +-- test_take
/// |               +-- test_put_back
/// |           +-- test_ban
/// |               +-- test_address_banned_fail
/// |               +-- test_different_account_fail
/// |               +-- test_not_owned_balance_fail
/// ```
module ABC::Tests {
    use ABC::ABC::{Self, ABC, Registry};
    use RC::RegulatedCoin::{Self as RCoin, RegulatedCoin as RCoin};

    use Sui::Coin::{Coin, TreasuryCap};
    use Sui::TestScenario::{Self, Scenario, next_tx, ctx};

    // === Test handlers; this trick helps reusing scenarios ==

    #[test] public(script) fun test_minting() { test_minting_(&mut scenario()) }
    #[test] public(script) fun test_creation() { test_creation_(&mut scenario()) }
    #[test] public(script) fun test_transfer() { test_transfer_(&mut scenario()) }
    #[test] public(script) fun test_burn() { test_burn_(&mut scenario()) }
    #[test] public(script) fun test_take() { test_take_(&mut scenario()) }
    #[test] public(script) fun test_put_back() { test_put_back_(&mut scenario()) }
    #[test] public(script) fun test_ban() { test_ban_(&mut scenario()) }

    #[test]
    #[expected_failure(abort_code = 2)]
    public(script) fun test_address_banned_fail() {
        test_address_banned_fail_(&mut scenario())
    }

    #[test]
    #[expected_failure(abort_code = 2)]
    public(script) fun test_different_account_fail() {
        test_different_account_fail_(&mut scenario())
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    public(script) fun test_not_owned_balance_fail() {
        test_not_owned_balance_fail_(&mut scenario())
    }

    // === Helpers and basic test organization ===

    fun scenario(): Scenario { TestScenario::begin(&@ABC) }
    fun people(): (address, address, address) { (@0xABC, @0xE05, @0xFACE) }

    // Admin creates a regulated coin ABC and mints 1,000,000 of it.
    public(script) fun test_minting_(test: &mut Scenario) {
        let (admin, _, _) = people();

        next_tx(test, &admin); {
            ABC::init_for_testing(ctx(test))
        };

        next_tx(test, &admin); {
            let cap = TestScenario::take_owned<TreasuryCap<ABC>>(test);
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);

            ABC::mint(&mut cap, &mut coin, 1000000, ctx(test));

            assert!(RCoin::value(&coin) == 1000000, 0);

            TestScenario::return_owned(test, cap);
            TestScenario::return_owned(test, coin);
        }
    }

    // Admin creates an empty balance for the `user1`.
    public(script) fun test_creation_(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_minting_(test);

        next_tx(test, &admin); {
            let cap = TestScenario::take_owned<TreasuryCap<ABC>>(test);

            ABC::create(&cap, user1, ctx(test));

            TestScenario::return_owned(test, cap);
        };

        next_tx(test, &user1); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);

            assert!(RCoin::creator(&coin) == user1, 1);
            assert!(RCoin::value(&coin) == 0, 2);

            TestScenario::return_owned(test, coin);
        };
    }

    // Admin transfers 500,000 coins to `user1`.
    // User1 accepts the transfer and checks his balance.
    public(script) fun test_transfer_(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_creation_(test);

        next_tx(test, &admin); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);
            let reg = TestScenario::take_shared<Registry>(test);
            let reg_ref = TestScenario::borrow_mut(&mut reg);

            ABC::transfer(reg_ref, &mut coin, 500000, user1, ctx(test));

            TestScenario::return_shared(test, reg);
            TestScenario::return_owned(test, coin);
        };

        next_tx(test, &user1); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);
            let transfer = TestScenario::take_owned<ABC::Transfer>(test);
            let reg = TestScenario::take_shared<Registry>(test);
            let reg_ref = TestScenario::borrow_mut(&mut reg);

            ABC::accept_transfer(reg_ref, &mut coin, transfer, ctx(test));

            assert!(RCoin::value(&coin) == 500000, 3);

            TestScenario::return_shared(test, reg);
            TestScenario::return_owned(test, coin);
        };
    }

    // Admin burns 100,000 of `RCoin<ABC>`
    public(script) fun test_burn_(test: &mut Scenario) {
        let (admin, _, _) = people();

        test_transfer_(test);

        next_tx(test, &admin); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);
            let treasury_cap = TestScenario::take_owned<TreasuryCap<ABC>>(test);

            ABC::burn(&mut treasury_cap, &mut coin, 100000, ctx(test));

            assert!(RCoin::value(&coin) == 400000, 4);

            TestScenario::return_owned(test, treasury_cap);
            TestScenario::return_owned(test, coin);
        };
    }

    // User1 cashes 100,000 of his `RegulatedCoin` into a `Coin`;
    // User1 sends Coin<ABC> it to `user2`.
    public(script) fun test_take_(test: &mut Scenario) {
        let (_, user1, user2) = people();

        test_transfer_(test);

        next_tx(test, &user1); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);
            let reg = TestScenario::take_shared<Registry>(test);
            let reg_ref = TestScenario::borrow_mut(&mut reg);

            ABC::take(reg_ref, &mut coin, 100000, ctx(test));

            assert!(ABC::swapped_amount(reg_ref) == 100000, 5);
            assert!(RCoin::value(&coin) == 400000, 5);

            TestScenario::return_shared(test, reg);
            TestScenario::return_owned(test, coin);
        };

        next_tx(test, &user1); {
            let coin = TestScenario::take_owned<Coin<ABC>>(test);
            Sui::Transfer::transfer(coin, user2);
        };
    }

    // User2 sends his `Coin<ABC>` to `admin`.
    // Admin puts this coin to his RegulatedCoin balance.
    public(script) fun test_put_back_(test: &mut Scenario) {
        let (admin, _, user2) = people();

        test_take_(test);

        next_tx(test, &user2); {
            let coin = TestScenario::take_owned<Coin<ABC>>(test);
            Sui::Transfer::transfer(coin, admin);
        };

        next_tx(test, &admin); {
            let coin = TestScenario::take_owned<Coin<ABC>>(test);
            let reg_coin = TestScenario::take_owned<RCoin<ABC>>(test);
            let reg = TestScenario::take_shared<Registry>(test);
            let reg_ref = TestScenario::borrow_mut(&mut reg);

            ABC::put_back(reg_ref, &mut reg_coin, coin, ctx(test));

            TestScenario::return_owned(test, reg_coin);
            TestScenario::return_shared(test, reg);
        }
    }

    // Admin bans user1 by adding his address to the registry.
    public(script) fun test_ban_(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_transfer_(test);

        next_tx(test, &admin); {
            let cap = TestScenario::take_owned<TreasuryCap<ABC>>(test);
            let reg = TestScenario::take_shared<Registry>(test);
            let reg_ref = TestScenario::borrow_mut(&mut reg);

            ABC::ban(&cap, reg_ref, user1, ctx(test));

            TestScenario::return_shared(test, reg);
            TestScenario::return_owned(test, cap);
        };
    }

    // Banned User1 fails to create a Transfer.
    public(script) fun test_address_banned_fail_(test: &mut Scenario) {
        let (_, user1, user2) = people();

        test_ban_(test);

        next_tx(test, &user1); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);
            let reg = TestScenario::take_shared<Registry>(test);
            let reg_ref = TestScenario::borrow_mut(&mut reg);

            ABC::transfer(reg_ref, &mut coin, 250000, user2, ctx(test));

            TestScenario::return_shared(test, reg);
            TestScenario::return_owned(test, coin);
        };
    }

    // User1 is banned. Admin tries to make a Transfer to User1 and fails - user banned.
    public(script) fun test_different_account_fail_(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_ban_(test);

        next_tx(test, &admin); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);
            let reg = TestScenario::take_shared<Registry>(test);
            let reg_ref = TestScenario::borrow_mut(&mut reg);

            ABC::transfer(reg_ref, &mut coin, 250000, user1, ctx(test));

            TestScenario::return_shared(test, reg);
            TestScenario::return_owned(test, coin);
        };
    }

    // User1 is banned and transfers the whole balance to User2.
    // User2 tries to use this balance and fails.
    public(script) fun test_not_owned_balance_fail_(test: &mut Scenario) {
        let (_, user1, user2) = people();

        test_ban_(test);

        next_tx(test, &user1); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);
            Sui::Transfer::transfer(coin, user2);
        };

        next_tx(test, &user2); {
            let coin = TestScenario::take_owned<RCoin<ABC>>(test);
            let reg = TestScenario::take_shared<Registry>(test);
            let reg_ref = TestScenario::borrow_mut(&mut reg);

            ABC::transfer(reg_ref, &mut coin, 500000, user1, ctx(test));

            TestScenario::return_shared(test, reg);
            TestScenario::return_owned(test, coin);
        }
    }
}
