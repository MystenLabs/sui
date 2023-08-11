// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


#[allow(unused_field)]
/// This is an implementation approach for Closed-Loop Tokens. The idea behind it is that the token
/// behaves like a coin but is not freely available, and every action with this coin is permissioned.
///
/// The rules applied to the token are defined by the token issuer and include:
///
/// - mint / burn
/// - transfer
/// - split
/// - merge
/// - spend (merge with a shared balance)
/// - shared balance
/// - convert to Coin
/// - convert from Coin
///
/// The Sui Framework already has a Balance<T> primitive which seems like the best base to utilize
/// for an experiment like this. Supply<T> will control the supply, however, in this implemntation
/// we will use some practices that may not be ideal under different circumstances, such as issuing
/// multiple supplies.
///
/// Another important side of the approach is it being similar to the TransferPolicy implementation
/// that makes Kiosk trades secure and approvable by the creator always. Utilizing Hot Potato to
/// guarantee that every action is protected and approved by the issuer is a decent pattern.
///
/// Notes:
///
/// - We can't use OTW for the purpose to give space to Publisher.
/// - We will intentionally utilize multiple `Supply`s to allow multiple policies.
/// - CL stands for Closed-Loop.
/// - We're using TempToken to abstract away from ownership.
module fungible_tokens::closed_loop {
    use std::type_name::{Self, TypeName};
    use sui::balance::{Self, Balance, Supply};
    use sui::object::{Self, ID, UID};
    use sui::vec_set::{Self, VecSet};
    use sui::tx_context::TxContext;
    use sui::coin::{Self, Coin};

    /// Trying to use not one-time witness.
    const ENotOneTime: u64 = 0;
    /// Trying to resolve an action which is not allowed.
    const ENotAllowed: u64 = 1;
    /// Allowing an action that is already allowed.
    const EAlreadyExists: u64 = 2;
    /// Trying to spend more than the balance.
    const ENotEnough: u64 = 3;
    /// For the functions that are being designed.
    const ENotImplemented: u64 = 1337;

    // === Policy and Cap ===

    /// The Capability that grants the owner the ability to create new TokenPolicy instances.
    struct CoinIssuerCap<phantom T> has key, store {
        id: UID,
        policy_id: ID
    }

    /// A policy that defines the rules for a specific token.
    struct CLPolicy<phantom T> has key, store {
        id: UID,
        /// NOTE: depending on whether we want to allow multiple policies, we can choose to use the
        /// TreasuryCap here.
        supply: Supply<T>,
        allowed_actions: VecSet<TypeName>
    }

    // === Token and it's temporary state ===

    /// A single OWNED token. It is important to differentiate shared and owned balances to split
    /// the functionalities and provide guarantees for both scenarios.
    // struct CLToken<phantom T> has key {
    //     id: UID,
    //     balance: Balance<T>
    // }

    /// A temporary struct which is used in between operations.
    /// We use it to generalize Owned and Shared balance operations.
    struct TempToken<phantom T> {
        balance: Balance<T>
    }

    // === Actions ===

    /// A single permission that can be granted to a token.
    struct ActionRequest<phantom T, phantom Action> {
        /// The amount of the CLToken that is being operated on.
        amount: u64,
        // Whether the request was resolved using a 3rd party functionality.
        // external: bool
    }

    // I really want an enum...

    /// GENERAL: The action of minting a new token.
    struct Mint {}
    /// GENERAL: The action of burning an existing token.
    struct Burn {}
    /// GENERAL: The action of splitting an existing token into two.
    struct Split {}
    /// GENERAL: The action of joining two existing tokens into one.
    struct Join {}

    /// STORAGE: The action of merging an existing token with a shared balance.
    struct Spend {}
    /// STORAGE: The action of transferring an existing token.
    struct Transfer {}

    /// CONVERSION: The action of converting a token into a Coin.
    struct ToCoin {}
    /// CONVERSION: The action of converting a Coin into a token.
    struct FromCoin {}

    // === Creator Actions ===

    /// Create a new policy and the capability to control it.
    public fun new_token<T: drop>(otw: T, ctx: &mut TxContext): (CLPolicy<T>, CoinIssuerCap<T>) {
        assert!(sui::types::is_one_time_witness(&otw), ENotOneTime);

        let policy_uid = object::new(ctx);
        let policy_id = object::uid_to_inner(&policy_uid);

        (
            CLPolicy { id: policy_uid, supply: balance::create_supply(otw), allowed_actions: vec_set::empty() },
            CoinIssuerCap { id: object::new(ctx), policy_id }
        )
    }

    // === Token Operations ===

    /// Mint a new token.
    public fun mint<T>(policy: &mut CLPolicy<T>, amount: u64, _ctx: &mut TxContext): (TempToken<T>, ActionRequest<T, Mint>) {
        let balance = balance::increase_supply(&mut policy.supply, amount);
        let token = TempToken { balance };

        (token, ActionRequest { amount })
    }

    /// Burn an existing token.
    public fun burn<T>(policy: &mut CLPolicy<T>, token: TempToken<T>, _ctx: &mut TxContext): ActionRequest<T, Burn> {
        let TempToken { balance } = token;
        let amount = balance::decrease_supply(&mut policy.supply, balance);

        ActionRequest { amount }
    }

    /// Split an existing token.
    public fun split<T>(token: &mut TempToken<T>, amount: u64, _ctx: &mut TxContext): (TempToken<T>, ActionRequest<T, Split>) {
        assert!(value(token) >= amount, ENotEnough);

        let TempToken { balance } = token;
        let balance = balance::split(balance, amount);
        let token = TempToken { balance };

        (token, ActionRequest { amount })
    }

    /// Join two existing tokens into one. The request will be for the resulting amount.
    public fun join<T>(token: &mut TempToken<T>, another: TempToken<T>, _ctx: &mut TxContext): ActionRequest<T, Join> {
        let TempToken { balance } = another;
        let amount = balance::join(&mut token.balance, balance);

        ActionRequest { amount }
    }

    // === Ownership and storage models ===

    /// Transfer an existing token (without splitting!)
    public fun transfer<T>(_token: TempToken<T>, _to: address, _ctx: &mut TxContext): ActionRequest<T, Transfer> {
        // let amount = value(&token);
        // sui::transfer::transfer(token, to);
        // ActionRequest<Transfer> { amount };

        abort ENotImplemented
    }



    // === Danger Zone - Coin Conversion ===

    /// Convert a token to a Coin.
    public fun to_coin<T>(token: TempToken<T>, ctx: &mut TxContext): (Coin<T>, ActionRequest<T, ToCoin>) {
        let TempToken { balance } = token;
        let amount = balance::value(&balance);

        (coin::from_balance(balance, ctx), ActionRequest { amount })
    }

    /// Convert a Coin to a token.
    public fun from_coin<T>(coin: Coin<T>, _ctx: &mut TxContext): (TempToken<T>, ActionRequest<T, FromCoin>) {
        let balance = coin::into_balance(coin);
        let amount = balance::value(&balance);
        let token = TempToken { balance };

        (token, ActionRequest { amount })
    }

    // === ActionRequest resolution ===

    /// Resolve an action request if it is allowed.
    public fun resolve<T, Action>(policy: &mut CLPolicy<T>, req: ActionRequest<T, Action>) {
        assert!(vec_set::contains(&policy.allowed_actions, &type_name::get<Action>()), ENotAllowed);
        let ActionRequest { amount: _ } = req;
    }

    /// Allow an action to be resolved in the policy.
    public fun allow<T, Action>(_cap: &CoinIssuerCap<T>, policy: &mut CLPolicy<T>) {
        let type_name = type_name::get<Action>();

        assert!(!vec_set::contains(&policy.allowed_actions, &type_name), EAlreadyExists);
        vec_set::insert(&mut policy.allowed_actions, type_name);
    }

    /// Only for owner (or custom logic). Resolves any request.
    public fun resolve_as_owner<T, Action>(_cap: &CoinIssuerCap<T>, req: ActionRequest<T, Action>) {
        let ActionRequest { amount: _ } = req;
    }

    // === Reads ===

    /// Read the value of the token.
    public fun value<T>(token: &TempToken<T>): u64 { balance::value(&token.balance) }


    // Open questions:
    //
    // - How to solve mint / burn requests and how to delegate them to a third party.
    // - Should split / merge be protected? In my opinion - yes, but wonder if there's a caveat to this.
    // - Which ownership types do we want to allow. I'm pro everything altogether.
}

#[test_only]
module fungible_tokens::bonus {
    use sui::tx_context::{sender, TxContext};
    use sui::object;

    use fungible_tokens::closed_loop;

    struct BONUS has drop {}

    /// Create a new Token with a policy. Allow transfers.
    fun init(otw: BONUS, ctx: &mut TxContext) {
        let (policy, cap) = closed_loop::new_token(otw, ctx);



        let token = closed_loop::mint(policy, 100, ctx);
        closed_loop::transfer(token, to, ctx);
    }

}
