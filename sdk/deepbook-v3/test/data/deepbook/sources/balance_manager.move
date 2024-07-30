// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The BalanceManager is a shared object that holds all of the balances for different assets. A combination of `BalanceManager` and
/// `TradeProof` are passed into a pool to perform trades. A `TradeProof` can be generated in two ways: by the
/// owner directly, or by any `TradeCap` owner. The owner can generate a `TradeProof` without the risk of
/// equivocation. The `TradeCap` owner, due to it being an owned object, risks equivocation when generating
/// a `TradeProof`. Generally, a high frequency trading engine will trade as the default owner.
module deepbook::balance_manager {
    // === Imports ===
    use sui::{
        bag::{Self, Bag},
        balance::{Self, Balance},
        coin::Coin,
        vec_set::{Self, VecSet},
    };

    // === Errors ===
    const EInvalidOwner: u64 = 0;
    const EInvalidTrader: u64 = 1;
    const EInvalidProof: u64 = 2;
    const EBalanceManagerBalanceTooLow: u64 = 3;
    const EMaxTradeCapsReached: u64 = 4;
    const ETradeCapNotInList: u64 = 5;

    // === Constants ===
    const MAX_TRADE_CAPS: u64 = 1000;

    // === Structs ===
    /// A shared object that is passed into pools for placing orders.
    public struct BalanceManager has key {
        id: UID,
        owner: address,
        balances: Bag,
        allow_listed: VecSet<ID>,
    }

    /// Balance identifier.
    public struct BalanceKey<phantom T> has store, copy, drop {}

    /// Owners of a `TradeCap` need to get a `TradeProof` to trade across pools in a single PTB (drops after).
    public struct TradeCap has key, store {
        id: UID,
        balance_manager_id: ID,
    }

    /// BalanceManager owner and `TradeCap` owners can generate a `TradeProof`.
    /// `TradeProof` is used to validate the balance_manager when trading on DeepBook.
    public struct TradeProof has drop {
        balance_manager_id: ID,
        trader: address,
    }

    // === Public-Mutative Functions ===
    public fun new(ctx: &mut TxContext): BalanceManager {
        BalanceManager {
            id: object::new(ctx),
            owner: ctx.sender(),
            balances: bag::new(ctx),
            allow_listed: vec_set::empty(),
        }
    }

    #[allow(lint(share_owned))]
    public fun share(balance_manager: BalanceManager) {
        transfer::share_object(balance_manager);
    }

    /// Returns the balance of a Coin in an balance_manager.
    public fun balance<T>(balance_manager: &BalanceManager): u64 {
        let key = BalanceKey<T> {};
        if (!balance_manager.balances.contains(key)) {
            0
        } else {
            let acc_balance: &Balance<T> = &balance_manager.balances[key];
            acc_balance.value()
        }
    }

    /// Mint a `TradeCap`, only owner can mint a `TradeCap`.
    public fun mint_trade_cap(balance_manager: &mut BalanceManager, ctx: &mut TxContext): TradeCap {
        balance_manager.validate_owner(ctx);
        assert!(balance_manager.allow_listed.size() < MAX_TRADE_CAPS, EMaxTradeCapsReached);

        let id = object::new(ctx);
        balance_manager.allow_listed.insert(id.to_inner());

        TradeCap {
            id,
            balance_manager_id: object::id(balance_manager),
        }
    }

    /// Revoke a `TradeCap`. Only the owner can revoke a `TradeCap`.
    public fun revoke_trade_cap(balance_manager: &mut BalanceManager, trade_cap_id: &ID, ctx: &TxContext) {
        balance_manager.validate_owner(ctx);

        assert!(balance_manager.allow_listed.contains(trade_cap_id), ETradeCapNotInList);
        balance_manager.allow_listed.remove(trade_cap_id);
    }

    /// Generate a `TradeProof` by the owner. The owner does not require a capability
    /// and can generate TradeProofs without the risk of equivocation.
    public fun generate_proof_as_owner(balance_manager: &mut BalanceManager, ctx: &TxContext): TradeProof {
        balance_manager.validate_owner(ctx);

        TradeProof {
            balance_manager_id: object::id(balance_manager),
            trader: ctx.sender(),
        }
    }

    /// Generate a `TradeProof` with a `TradeCap`.
    /// Risk of equivocation since `TradeCap` is an owned object.
    public fun generate_proof_as_trader(balance_manager: &mut BalanceManager, trade_cap: &TradeCap, ctx: &TxContext): TradeProof {
        balance_manager.validate_trader(trade_cap);

        TradeProof {
            balance_manager_id: object::id(balance_manager),
            trader: ctx.sender(),
        }
    }

    /// Deposit funds to an balance_manager. Only owner can call this directly.
    public fun deposit<T>(
        balance_manager: &mut BalanceManager,
        coin: Coin<T>,
        ctx: &mut TxContext,
    ) {
        let proof = generate_proof_as_owner(balance_manager, ctx);

        balance_manager.deposit_with_proof(&proof, coin.into_balance());
    }

    /// Withdraw funds from an balance_manager. Only owner can call this directly.
    /// If withdraw_all is true, amount is ignored and full balance withdrawn.
    /// If withdraw_all is false, withdraw_amount will be withdrawn.
    public fun withdraw<T>(
        balance_manager: &mut BalanceManager,
        withdraw_amount: u64,
        ctx: &mut TxContext,
    ): Coin<T> {
        let proof = generate_proof_as_owner(balance_manager, ctx);

        balance_manager.withdraw_with_proof(&proof, withdraw_amount, false).into_coin(ctx)
    }

    public fun withdraw_all<T>(
        balance_manager: &mut BalanceManager,
        ctx: &mut TxContext,
    ): Coin<T> {
        let proof = generate_proof_as_owner(balance_manager, ctx);

        balance_manager.withdraw_with_proof(&proof, 0, true).into_coin(ctx)
    }

    public fun validate_proof(balance_manager: &BalanceManager, proof: &TradeProof) {
        assert!(object::id(balance_manager) == proof.balance_manager_id, EInvalidProof);
    }

    /// Returns the owner of the balance_manager.
    public fun owner(balance_manager: &BalanceManager): address {
        balance_manager.owner
    }

    /// Returns the owner of the balance_manager.
    public fun id(balance_manager: &BalanceManager): ID {
        balance_manager.id.to_inner()
    }

    // === Public-Package Functions ===
    /// Deposit funds to an balance_manager. Pool will call this to deposit funds.
    public(package) fun deposit_with_proof<T>(
        balance_manager: &mut BalanceManager,
        proof: &TradeProof,
        to_deposit: Balance<T>,
    ) {
        balance_manager.validate_proof(proof);

        let key = BalanceKey<T> {};

        if (balance_manager.balances.contains(key)) {
            let balance: &mut Balance<T> = &mut balance_manager.balances[key];
            balance.join(to_deposit);
        } else {
            balance_manager.balances.add(key, to_deposit);
        }
    }

    /// Withdraw funds from an balance_manager. Pool will call this to withdraw funds.
    public(package) fun withdraw_with_proof<T>(
        balance_manager: &mut BalanceManager,
        proof: &TradeProof,
        withdraw_amount: u64,
        withdraw_all: bool,
    ): Balance<T> {
        balance_manager.validate_proof(proof);

        let key = BalanceKey<T> {};
        let key_exists = balance_manager.balances.contains(key);
        if (withdraw_all) {
            if (key_exists) {
                balance_manager.balances.remove(key)
            } else {
                balance::zero()
            }
        } else {
            assert!(key_exists, EBalanceManagerBalanceTooLow);
            let acc_balance: &mut Balance<T> = &mut balance_manager.balances[key];
            let acc_value = acc_balance.value();
            assert!(acc_value >= withdraw_amount, EBalanceManagerBalanceTooLow);
            if (withdraw_amount == acc_value) {
                balance_manager.balances.remove(key)
            } else {
                acc_balance.split(withdraw_amount)
            }
        }
    }

    /// Deletes an balance_manager.
    /// This is used for deleting temporary balance_managers for direct swap with pool.
    public(package) fun delete(balance_manager: BalanceManager) {
        let BalanceManager {
            id,
            owner: _,
            balances,
            allow_listed: _,
        } = balance_manager;

        id.delete();
        balances.destroy_empty();
    }

    public(package) fun trader(trade_proof: &TradeProof): address {
        trade_proof.trader
    }

    // === Private Functions ===
    fun validate_owner(balance_manager: &BalanceManager, ctx: &TxContext) {
        assert!(ctx.sender() == balance_manager.owner(), EInvalidOwner);
    }

    fun validate_trader(balance_manager: &BalanceManager, trade_cap: &TradeCap) {
        assert!(balance_manager.allow_listed.contains(object::borrow_id(trade_cap)), EInvalidTrader);
    }
}