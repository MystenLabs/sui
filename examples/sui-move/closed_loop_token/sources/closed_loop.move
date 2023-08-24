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
/// - create zero
/// - destroy zero
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
module closed_loop::closed_loop {
    use std::option::{Self, Option};
    use std::type_name::{Self, TypeName};
    use sui::balance::{Self, Balance, Supply};
    use sui::object::{Self, ID, UID};
    use sui::vec_set::{Self, VecSet};
    use sui::tx_context::{sender, TxContext};
    use sui::coin::{Self, Coin};

    /// Trying to use not one-time witness.
    const ENotOneTime: u64 = 0;
    /// Trying to resolve an action which is not allowed.
    const ENotAllowed: u64 = 1;
    /// Allowing an action that is already allowed.
    const EAlreadyExists: u64 = 2;
    /// Trying to spend more than the balance.
    const ENotEnough: u64 = 3;
    /// Trying to use a disabled resolver.
    const EResolverDisabled: u64 = 4;
    /// Trying to process more than the single limit.
    const ESingleLimitExceeded: u64 = 5;
    /// Trying to process more than the total limit.
    const ETotalLimitExceeded: u64 = 6;

    // === Policy and Cap ===

    /// The Capability that grants the owner the ability to create new TokenPolicy instances.
    struct CoinIssuerCap<phantom T> has key, store {
        id: UID,
        policy_id: ID
    }

    /// A policy that defines the rules for a specific token.
    ///
    /// Similar to `TreasuryCap<T>`
    struct CLPolicy<phantom T> has key, store {
        id: UID,
        /// NOTE: depending on whether we want to allow multiple policies, we can choose to use the
        /// TreasuryCap here.
        supply: Supply<T>,
        /// A list of currently active resolvers for this policy. Resolvers can
        /// be disabled at any moment by removing them from this `VecSet`.
        /// Added via the `create_resolver` function.
        custom_resolvers: VecSet<ID>,
        /// A list of actions that are allowed to be resolved by default. Added
        /// via the `allow` function.
        allowed_actions: VecSet<TypeName>
    }

    /// A resolver that can be used to resolve a specific action.
    /// Comes with a configuration:
    /// - single limit - max amount that can be processed in a single operation
    /// - total limit  - max amount that can be processed in total by the Resolver
    struct Resolver<phantom T, phantom Action> has store, drop {
        id: ID,
        /// Total limit of Tokens that can be processed by this resolver in total.
        total_limit: Option<u64>,
        /// Max amount of a single Token that can be processed by this resolver.
        single_limit: Option<u64>,
        /// Sum of amounts of Tokens that have been processed by this resolver.
        processed: u64
    }

    // === Storage Models ===

    /// A single owner Token, similar to Coin<T> but without `store`.
    struct Token<phantom T> has key {
        id: UID,
        balance: Balance<T>
    }

    // === Token and it's temporary state ===

    /// A temporary struct which is used in between operations.
    /// We use it to generalize Owned and Shared balance operations.
    struct TempToken<phantom T> {
        /// Balance of the token, similar to Coin<T>
        balance: Balance<T>
    }

    // === Actions ===

    /// A request for an action to be resolved; whenever any of the action
    /// functions are called, they return an `ActionRequest` which can must
    /// resolved either in the Policy using the `resolve_default` function or
    /// in a custom resolver using the `resolve_custom` function.
    struct ActionRequest<phantom T, phantom Action> {
        /// The amount of the Token that is being operated on.
        amount: u64,
    }

    // === Action Types ===

    struct Mint {}      // GENERAL: The action of minting a new token.
    struct Burn {}      // GENERAL: The action of burning an existing token.
    struct Split {}     // GENERAL: The action of splitting an existing token into two.
    struct Join {}      // GENERAL: The action of joining two existing tokens into one.

    struct Spend {}     // STORAGE: The action of merging an existing token with a shared balance.
    struct Transfer {}  // STORAGE: The action of transferring an existing token.

    struct ToCoin {}    // CONVERSION: The action of converting a token into a Coin.
    struct FromCoin {}  // CONVERSION: The action of converting a Coin into a token.

    struct Zero {}        // UTILITY: The action of creating a zero Token.
    struct DestroyZero {} // UTILITY: The action of destroying a zero Token.

    // === Creator Actions ===

    /// Create a new policy and the capability to control it.
    public fun new_token<T: drop>(otw: T, ctx: &mut TxContext): (CLPolicy<T>, CoinIssuerCap<T>) {
        assert!(sui::types::is_one_time_witness(&otw), ENotOneTime);

        let policy_uid = object::new(ctx);
        let policy_id = object::uid_to_inner(&policy_uid);

        (
            CLPolicy {
                id: policy_uid,
                supply: balance::create_supply(otw),
                allowed_actions: vec_set::empty(),
                custom_resolvers: vec_set::empty()
            },
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

    /// Create a zero balance token.
    public fun zero<T>(_ctx: &mut TxContext): (TempToken<T>, ActionRequest<T, Zero>) {
        (TempToken { balance: balance::zero() }, ActionRequest { amount: 0 })
    }

    /// Destroy zero balance token.
    public fun destroy_zero<T>(token: TempToken<T>, _ctx: &mut TxContext): ActionRequest<T, DestroyZero> {
        let TempToken { balance } = token;
        balance::destroy_zero(balance);
        ActionRequest { amount: 0 }
    }

    // === Ownership and storage models ===

    /// Create a temporary token from an owned one.
    public fun temp_from_owned<T>(owned: Token<T>, _ctx: &mut TxContext): TempToken<T> {
        let Token { id, balance } = owned;
        object::delete(id);
        TempToken { balance }
    }

    /// Create an owned token from a temporary one.
    public fun temp_into_owned<T>(token: TempToken<T>, ctx: &mut TxContext) {
        let TempToken { balance } = token;
        let id = object::new(ctx);

        sui::transfer::transfer(Token { id, balance }, sender(ctx));
    }

    /// Transfer an existing token (without splitting!)
    public fun transfer<T>(token: TempToken<T>, to: address, ctx: &mut TxContext): ActionRequest<T, Transfer> {
        let TempToken { balance } = token;
        let amount = balance::value(&balance);
        let owned = Token { id: object::new(ctx), balance };

        sui::transfer::transfer(owned, to);
        ActionRequest { amount }
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

    // === ActionRequest Resolution ===

    /// Resolve an action request if it is allowed.
    public fun resolve_default<T, Action>(policy: &mut CLPolicy<T>, req: ActionRequest<T, Action>) {
        assert!(vec_set::contains(&policy.allowed_actions, &type_name::get<Action>()), ENotAllowed);
        let ActionRequest { amount: _ } = req;
    }

    /// Allow an action to be resolved in the policy - allowed to everyone.
    public fun allow<T, Action>(_cap: &CoinIssuerCap<T>, policy: &mut CLPolicy<T>) {
        let type_name = type_name::get<Action>();

        assert!(!vec_set::contains(&policy.allowed_actions, &type_name), EAlreadyExists);
        vec_set::insert(&mut policy.allowed_actions, type_name);
    }

    // === Custom Resolution ===

    /// Create a custom resolver for a policy (unlike default when an action is allowed in the policy by default).
    public fun create_resolver<T, Action>(
        _cap: &CoinIssuerCap<T>,
        policy: &mut CLPolicy<T>,
        single_limit: Option<u64>,
        total_limit: Option<u64>,
        ctx: &mut TxContext
    ): Resolver<T, Action> {
        let id = object::id_from_address(sui::tx_context::fresh_object_address(ctx));
        vec_set::insert(&mut policy.custom_resolvers, id);
        Resolver { id, single_limit, total_limit, processed: 0 }
    }

    /// Resolve an action request using a custom resolver.
    public fun resolve_custom<T, Action>(
        policy: &mut CLPolicy<T>,
        resolver: &mut Resolver<T, Action>,
        req: ActionRequest<T, Action>,
    ) {
        let ActionRequest { amount } = req;
        let processed = resolver.processed + amount;

        assert!(vec_set::contains(&policy.custom_resolvers, &resolver.id), EResolverDisabled);
        assert!(less_than_option(amount, &resolver.single_limit), ESingleLimitExceeded);
        assert!(less_than_option(processed, &resolver.total_limit), ETotalLimitExceeded);

        resolver.processed = processed;
    }

    /// Disable a custom resolver by removing it from the `custom_resolvers` list.
    public fun disable_resolver<T>(
        _cap: &CoinIssuerCap<T>,
        policy: &mut CLPolicy<T>,
        resolver_id: ID,
        _ctx: &mut TxContext
    ) {
        vec_set::remove(&mut policy.custom_resolvers, &resolver_id);
    }

    /// Only for owner (or custom logic). Resolves any request.
    public fun resolve_as_owner<T, Action>(_cap: &CoinIssuerCap<T>, req: ActionRequest<T, Action>) {
        let ActionRequest { amount: _ } = req;
    }

    // === Reads ===

    /// Get the value of the token.
    public fun value<T>(token: &TempToken<T>): u64 { balance::value(&token.balance) }

    /// Get the value of the action request.
    public fun action_value<T, Action>(action: &ActionRequest<T, Action>): u64 { action.amount }

    /// Get the total supply of the policy.
    public fun total_supply<T>(policy: &CLPolicy<T>): u64 { balance::supply_value(&policy.supply) }

    /// Get the processed amount of the resolver.
    public fun resolver_processed<T, Action>(resolver: &Resolver<T, Action>): u64 { resolver.processed }

    /// Get the total limit of the resolver.
    public fun resolver_total_limit<T, Action>(resolver: &Resolver<T, Action>): Option<u64> { resolver.total_limit }

    /// Get the single limit of the resolver.
    public fun resolver_single_limit<T, Action>(resolver: &Resolver<T, Action>): Option<u64> { resolver.single_limit }

    // === Internal ===

    /// If the Option is Some, then make sure that the value is less or equal
    /// to the stored value. If the Option is None, return true.
    fun less_than_option(value: u64, option: &Option<u64>): bool {
        if (option::is_some(option)) {
            value <= *option::borrow(option)
        } else {
            true
        }
    }
}
