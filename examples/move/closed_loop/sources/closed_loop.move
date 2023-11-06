// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module implements a closed loop `Token` which is guarded by a
/// `TokenPolicy`. The `TokenPolicy` defines the allowed actions that can be
/// performed on the `Token`, and for each action, it stores the set of `Rules`
/// that must be satisfied for the action to be performed.
///
/// Actions:
/// - `transfer` - transfer the `Token` to another account
/// - `spend` - "burn" the `Token` and store it in the `TokenPolicy`
/// - `to_coin` - convert the `Token` into an open `Coin`
/// - `from_coin` - convert an open `Coin` into a `Token`
module closed_loop::closed_loop {
    use std::vector;
    use std::string::{Self, String};
    use std::option::{Self, Option};
    use std::type_name::{Self, TypeName};
    use sui::tx_context::{Self, TxContext};
    use sui::coin::{Self, Coin, TreasuryCap};
    use sui::balance::{Self, Balance};
    use sui::object::{Self, ID, UID};
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::{Self, VecSet};
    use sui::dynamic_field as df;
    use sui::transfer;

    /// The action is not allowed (defined) in the policy.
    const EUnknownAction: u64 = 0;
    /// The rule was not approved.
    const ENotApproved: u64 = 1;
    /// Trying to perform an admin action with a wrong cap.
    const ENotAuthorized: u64 = 2;
    /// The balance is too low to perform the action.
    const EBalanceTooLow: u64 = 3;
    /// The balance is not zero.
    const ENotZero: u64 = 4;
    /// The balance is not zero when trying to confirm with `TransferPolicyCap`.
    const ECantConsumeBalance: u64 = 5;
    /// Trying to perform an owner-gated action without being the owner.
    const ENotOwner: u64 = 6;

    /// A Tag for the `spend` action.
    const SPEND: vector<u8> = b"spend";
    /// A Tag for the `transfer` action.
    const TRANSFER: vector<u8> = b"transfer";
    /// A Tag for the `to_coin` action.
    const TO_COIN: vector<u8> = b"to_coin";
    /// A Tag for the `from_coin` action.
    const FROM_COIN: vector<u8> = b"from_coin";

    /// A token with closed-loop restrictions set by the issuer
    struct Token<phantom T> has key {
        id: UID,
        /// The Balance of the `Token`.
        balance: Balance<T>,
        /// The owner of the `Token`. Defaults to `tx_context::sender`, however
        /// for the situations like `transfer` it is set to the recipient. This
        /// field should help prevent arbitrary transfers with "Transfer To
        /// Object" (TTO) feature when it's released.
        owner: address,
    }

    /// A Capability that allows managing the `TokenPolicy`s.
    struct TokenPolicyCap<phantom T> has key, store { id: UID, for: ID }

    /// `TokenPolicy` represents a set of rules that define what actions can be
    /// performed on a `Token` and which `Rules` must be satisfied for the
    /// transaction to succeeed.
    ///
    /// - For the sake of availability, `TokenPolicy` is a `key`-only object.
    /// - Each `TokenPolicy` is managed by a matching `TokenPolicyCap`.
    /// - For an action to become available, there needs to be a record in the
    /// `rules` VecMap. To allow an action to be performed freely, there's an
    /// `allow` function that can be called by the `TokenPolicyCap` owner.
    struct TokenPolicy<phantom T> has key {
        id: UID,
        /// The balance that is effectively spent by the user on the "spend"
        /// action. However, actual decrease of the supply can only be done by
        /// the `TreasuryCap` owner.
        ///
        /// This balance can never be withdrawn by anyone and can only be
        /// `flush`-ed by the Admin.
        spent_balance: Balance<T>,
        /// The set of rules that define what actions can be performed on the
        /// token. Each rule contains the set of `TypeName`s that must be
        /// received by the `ActionRequest` for the action to be performed.
        rules: VecMap<String, VecSet<TypeName>>
    }

    /// A request to perform an "Action" on a token. Stores the information
    /// about the performed action and must be consumed by the `confirm_request`
    /// function when the Rules are satisfied.
    struct ActionRequest<phantom T> {
        /// Name of the Action to look up in the Policy. Name can be one of the
        /// default actions: `transfer`, `spend`, `to_coin`, `from_coin` or a
        /// custom action.
        name: String,
        /// Amount is present in all of the txs
        amount: u64,
        /// Sender is a permanent field always
        sender: address,
        /// Recipient is only available in `transfer` action.
        recipient: Option<address>,
        /// The balance to be "spent" in the `TokenPolicy`, only available
        /// in the `spend` action.
        spent_balance: Option<Balance<T>>,
        /// Collected approvals (stamps) from completed `Rules`.
        approvals: VecSet<TypeName>,
    }

    /// Dynamic field key for the `TokenPolicy` to store the `Config` for a
    /// specific `Rule` for an action.
    struct RuleKey has store, copy, drop { rule: TypeName }

    /// Create a new `TokenPolicy` and a matching `TokenPolicyCap`.
    /// The `TokenPolicy` must then be shared using the `share_policy` method.
    public fun new<T>(
        _treasury_cap: &TreasuryCap<T>, ctx: &mut TxContext
    ): (TokenPolicy<T>, TokenPolicyCap<T>) {
        let policy = TokenPolicy {
            id: object::new(ctx),
            spent_balance: balance::zero(),
            rules: vec_map::empty()
        };

        let cap = TokenPolicyCap {
            id: object::new(ctx),
            for: object::id(&policy)
        };

        (policy, cap)
    }

    // === Protected Actions ===

    /// Transfer a `Token` to a `recipient`. Creates an `ActionRequest` for the
    /// "transfer" action.
    ///
    /// Aborts if the `Token.owner` is not the transaction sender.
    public fun transfer<T>(
        t: Token<T>, recipient: address, ctx: &mut TxContext
    ): ActionRequest<T> {
        assert!(t.owner == tx_context::sender(ctx), ENotOwner);
        let amount = balance::value(&t.balance);
        t.owner = recipient;
        transfer::transfer(t, recipient);

        new_request(
            string::utf8(TRANSFER),
            amount,
            option::some(recipient),
            option::none(),
            ctx
        )
    }

    /// Spend a `Token` by "burning" it and storing in the `ActionRequest` for
    /// the "spend" action.
    ///
    /// Aborts if the `Token.owner` is not the transaction sender.
    public fun spend<T>(t: Token<T>, ctx: &mut TxContext): ActionRequest<T> {
        let Token { id, balance, owner } = t;
        assert!(owner == tx_context::sender(ctx), ENotOwner);
        object::delete(id);
        new_request(
            string::utf8(SPEND),
            balance::value(&balance),
            option::none(),
            option::some(balance),
            ctx
        )
    }

    /// Convert `Token` into an open `Coin`. Creates an `ActionRequest` for the
    /// "to_coin" action.
    ///
    /// Aborts if the `Token.owner` is not the transaction sender.
    public fun to_coin<T>(
        t: Token<T>, ctx: &mut TxContext
    ): (Coin<T>, ActionRequest<T>) {
        let Token { id, balance, owner } = t;
        let amount = balance::value(&balance);
        assert!(owner == tx_context::sender(ctx), ENotOwner);
        object::delete(id);

        (
            coin::from_balance(balance, ctx),
            new_request(
                string::utf8(TO_COIN),
                amount,
                option::none(),
                option::none(),
                ctx
            )
        )
    }

    /// Convert an open `Coin` into a `Token`. Creates an `ActionRequest` for
    /// the "from_coin" action.
    public fun from_coin<T>(
        coin: Coin<T>, ctx: &mut TxContext
    ): (Token<T>, ActionRequest<T>) {
        let balance = coin::into_balance(coin);
        let amount = balance::value(&balance);
        let owner = tx_context::sender(ctx);
        let token = Token { id: object::new(ctx), balance, owner };

        (
            token,
            new_request(
                string::utf8(FROM_COIN),
                amount,
                option::none(),
                option::none(),
                ctx
            )
        )
    }

    // === Public Actions ===

    /// Join two `Token`s into one, always available.
    public fun join<T>(token: &mut Token<T>, another: Token<T>) {
        let Token { id, balance, owner } = another;
        assert!(token.owner == owner, ENotOwner);
        balance::join(&mut token.balance, balance);
        object::delete(id);
    }

    /// Split a `Token` into two, always publicly available.
    public fun split<T>(
        token: &mut Token<T>, amount: u64, ctx: &mut TxContext
    ): Token<T> {
        assert!(balance::value(&token.balance) >= amount, EBalanceTooLow);
        let balance = balance::split(&mut token.balance, amount);
        Token { id: object::new(ctx), balance, owner: token.owner }
    }

    /// Create a zero `Token`.
    public fun zero<T>(ctx: &mut TxContext): Token<T> {
        let owner = tx_context::sender(ctx);
        Token { id: object::new(ctx), balance: balance::zero(), owner }
    }

    /// Destroy an empty `Token`, fails if the balance is non-zero.
    /// Aborts if the `Token.balance` is not zero
    public fun destroy_zero<T>(token: Token<T>) {
        let Token { id, balance, owner: _ } = token;
        assert!(balance::value(&balance) == 0, ENotZero);
        balance::destroy_zero(balance);
        object::delete(id);
    }

    /// Transfer the `Token` to the transaction sender.
    /// Aborts if the `Token.owner` is not the transaction sender.
    public fun keep<T>(token: Token<T>, ctx: &mut TxContext) {
        assert!(token.owner == tx_context::sender(ctx), ENotOwner);
        transfer::transfer(token, tx_context::sender(ctx))
    }

    // === Request Handling ===

    /// Create a new request to be confirmed by the `TokenPolicy`.
    public fun new_request<T>(
        name: String,
        amount: u64,
        recipient: Option<address>,
        spent_balance: Option<Balance<T>>,
        ctx: &TxContext
    ): ActionRequest<T> {
        ActionRequest {
            name,
            amount,
            recipient,
            spent_balance,
            sender: tx_context::sender(ctx),
            approvals: vec_set::empty(),
        }
    }

    /// Confirm the request against the `TokenPolicy` and return the parameters
    /// of the request: (Name, Amount, Sender, Recipient).
    ///
    /// Cannot be used for `spend` and similar actions that deliver `spent_balance`
    /// to the `TokenPolicy`. For those actions use `confirm_request_mut`.
    public fun confirm_request<T>(
        policy: &TokenPolicy<T>,
        request: ActionRequest<T>,
        _ctx: &mut TxContext
    ): (String, u64, address, Option<address>) {
        assert!(vec_map::contains(&policy.rules, &request.name), EUnknownAction);
        assert!(option::is_none(&request.spent_balance), ECantConsumeBalance);

        let ActionRequest {
            name, approvals,
            spent_balance,
            amount, sender, recipient,
        } = request;

        option::destroy_none(spent_balance);

        let rules = &vec_set::into_keys(*vec_map::get(&policy.rules, &name));
        let rules_len = vector::length(rules);
        let i = 0;

        while (i < rules_len) {
            let rule = vector::borrow(rules, i);
            assert!(vec_set::contains(&approvals, rule), ENotApproved);
            i = i + 1;
        };

        (name, amount, sender, recipient)
    }

    /// Confirm the request against the `TokenPolicy` and return the parameters
    /// of the request: (Name, Amount, Sender, Recipient).
    ///
    /// Unlike `confirm_request` this function requires mutable access to the
    /// `TokenPolicy` and must be used on `spend` action.
    public fun confirm_request_mut<T>(
        policy: &mut TokenPolicy<T>,
        request: ActionRequest<T>,
        ctx: &mut TxContext
    ): (String, u64, address, Option<address>) {
        assert!(vec_map::contains(&policy.rules, &request.name), EUnknownAction);
        if (option::is_some(&request.spent_balance)) {
            balance::join(
                &mut policy.spent_balance,
                option::extract(&mut request.spent_balance)
            );
        };

        confirm_request(policy, request, ctx)
    }

    /// Confirm the request using the `TreasuryCap` as having access to it means
    /// that the caller has full rights to the `Token` / `Coin`.
    ///
    /// TODO: consider `&mut on TreasuryCap` as a preemptive measure and/or as
    /// a way to guarantee that `TreasuryCap` is not frozen.
    public fun confirm_with_treasury_cap<T>(
        treasury_cap: &mut TreasuryCap<T>,
        request: ActionRequest<T>,
        _ctx: &mut TxContext
    ): (String, u64, address, Option<address>) {
        let ActionRequest {
            name, amount, sender, recipient, approvals: _,
            spent_balance
        } = request;

        if (option::is_some(&spent_balance)) {
            let spent = option::destroy_some(spent_balance);
            balance::decrease_supply(coin::supply_mut(treasury_cap), spent);
        } else {
            option::destroy_none(spent_balance);
        };

        (name, amount, sender, recipient)
    }

    /// An alternative to `confirm_with_treasury_cap` that uses the `TokenPolicyCap`,
    /// as the owner of the `TokenPolicy` has full authorization over the `Token`.
    ///
    /// TODO: consider `&mut on TokenPolicyCap` as a preemptive measure and/or as
    /// a way to guarantee that `TokenPolicyCap` is not frozen.
    public fun confirm_with_policy_cap<T>(
        _policy_cap: &TokenPolicyCap<T>,
        request: ActionRequest<T>,
        _ctx: &mut TxContext
    ): (String, u64, address, Option<address>) {
        assert!(option::is_none(&request.spent_balance), ECantConsumeBalance);

        let ActionRequest {
            name, amount, sender, recipient, approvals: _, spent_balance
        } = request;

        option::destroy_none(spent_balance);

        (name, amount, sender, recipient)
    }

    /// Add an approval to the request. Requires a Rule Witness.
    public fun add_approval<T, W: drop>(
        _t: W, request: &mut ActionRequest<T>, _ctx: &mut TxContext
    ) {
        vec_set::insert(&mut request.approvals, type_name::get<W>())
    }

    // === Rule Config ===

    /// Add a `Config` for a `Rule` in the `TokenPolicy`. Some rules may require
    /// a `Config` to be set before the action can be added / used.
    public fun add_rule_config<T, Rule: drop, Config: store>(
        _rule: Rule,
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        config: Config,
        _ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        df::add(&mut self.id, key<Rule>(), config)
    }

    /// Remove a `Config` for a `Rule` in the `TokenPolicy`. Unlike the addition
    /// of a `Config`, the removal of a `Config` does not require a `Rule`
    /// witness to be removed.
    public fun remove_rule_config<T, Rule: drop, Config: store>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        _ctx: &mut TxContext
    ): Config {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        df::remove(&mut self.id, key<Rule>())
    }

    /// Check if a `Config` for a `Rule` is set in the `TokenPolicy`.
    public fun has_rule_config<T, Rule: drop>(self: &TokenPolicy<T>): bool {
        df::exists_<RuleKey>(&self.id, key<Rule>())
    }

    /// Check if a `Config` for a `Rule` is set in the `TokenPolicy` and that
    /// it matches the type provided.
    public fun has_rule_config_with_type<T, Rule: drop, Config: store>(
        self: &TokenPolicy<T>
    ): bool {
        df::exists_with_type<RuleKey, Config>(&self.id, key<Rule>())
    }

    /// Get a `Config` for a `Rule` in the `TokenPolicy`. Requires `Rule`
    /// witness, hence can only be read by the `Rule` itself.
    public fun rule_config<T, Rule: drop, Config: store>(
        _rule: Rule, self: &TokenPolicy<T>
    ): &Config {
        df::borrow(&self.id, key<Rule>())
    }

    /// IMPORTANT: double check the requirement for mutability for Rules and
    /// consider a design that will allow for a Rule to mutate the Config in the
    /// future.
    public fun rule_config_mut<T, Rule: drop, Config: store>(
        _rule: Rule, self: &mut TokenPolicy<T>, cap: &TokenPolicyCap<T>
    ): &mut Config {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        df::borrow_mut(&mut self.id, key<Rule>())
    }

    // === Protected: Setting Rules ===

    /// Allows an action to be performed on the `Token` freely.
    public fun allow<T>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        _ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        vec_map::insert(&mut self.rules, action, vec_set::empty());
    }

    /// Completely disallows an action on the `Token`.
    public fun disallow<T>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        _ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        vec_map::remove(&mut self.rules, &action);
    }

    /// Adds a rule for an action with `name` in the `TokenPolicy`.
    public fun add_rule_for_action<T, Rule: drop>(
        _rule: Rule,
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        if (!vec_map::contains(&self.rules, &action)) {
            allow(self, cap, action, ctx);
        };

        vec_set::insert(
            vec_map::get_mut(&mut self.rules, &action),
            type_name::get<Rule>()
        )
    }

    /// Removes a rule for an action with `name` in the `TokenPolicy`. Returns
    /// the config object to be handled by the sender (or a Rule itself).
    public fun remove_rule_for_action<T, Rule: drop>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        _ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);

        vec_set::remove(
            vec_map::get_mut(&mut self.rules, &action),
            &type_name::get<Rule>()
        )
    }

    // === Protected: Minting and Burning ===

    /// Mint a `Token` with a given `amount` using the `TreasuryCap`.
    public fun mint<T>(
        cap: &mut TreasuryCap<T>, amount: u64, ctx: &mut TxContext
    ): Token<T> {
        let balance = balance::increase_supply(coin::supply_mut(cap), amount);
        Token { id: object::new(ctx), balance, owner: tx_context::sender(ctx) }
    }

    /// Burn a `Token` using the `TreasuryCap`.
    public fun burn<T>(
        cap: &mut TreasuryCap<T>, token: Token<T>
    ) {
        let Token { id, balance, owner: _ } = token;
        balance::decrease_supply(coin::supply_mut(cap), balance);
        object::delete(id);
    }

    /// A utility action - flush the burned balance and correct the supply in
    /// the `TreasuryCap`.
    public fun flush<T>(
        self: &mut TokenPolicy<T>,
        cap: &mut TreasuryCap<T>,
        _ctx: &mut TxContext
    ): u64 {
        let amount = balance::value(&self.spent_balance);
        let balance = balance::split(&mut self.spent_balance, amount);
        balance::decrease_supply(coin::supply_mut(cap), balance)
    }

    /// Every `TokenPolicy` must be shared in the end.
    public fun share_policy<T>(policy: TokenPolicy<T>) {
        transfer::share_object(policy)
    }

    // === Public Getters ===

    /// Check whether an action is present in the rules VecMap.
    public fun is_allowed<T>(self: &TokenPolicy<T>, action: &String): bool {
        vec_map::contains(&self.rules, action)
    }

    /// Returns the rules required for a specific action.
    public fun rules<T>(self: &TokenPolicy<T>, action: &String): VecSet<TypeName> {
        *vec_map::get(&self.rules, action)
    }

    /// Returns the `spent_balance` of the `TokenPolicy`.
    public fun spent_balance<T>(self: &TokenPolicy<T>): u64 {
        balance::value(&self.spent_balance)
    }

    /// Returns the `balance` of the `Token`.
    public fun value<T>(t: &Token<T>): u64 {
        balance::value(&t.balance)
    }

    // === Action Names ===

    /// Name of the Transfer action.
    public fun transfer_action(): String { string::utf8(TRANSFER) }

    /// Name of the `Spend` action.
    public fun spend_action(): String { string::utf8(SPEND) }

    /// Name of the `ToCoin` action.
    public fun to_coin_action(): String { string::utf8(TO_COIN) }

    /// Name of the `FromCoin` action.
    public fun from_coin_action(): String { string::utf8(FROM_COIN) }

    // === Action Request Fields ===

    /// Name of the `ActionRequest`.
    public fun name<T>(self: &ActionRequest<T>): String { self.name }

    /// Amount of the `ActionRequest`.
    public fun amount<T>(self: &ActionRequest<T>): u64 { self.amount }

    /// Sender of the `ActionRequest`.
    public fun sender<T>(self: &ActionRequest<T>): address { self.sender }

    /// Recipient of the `ActionRequest`.
    public fun recipient<T>(self: &ActionRequest<T>): Option<address> {
        self.recipient
    }

    /// Burned balance of the `ActionRequest`.
    public fun spent<T>(self: &ActionRequest<T>): Option<u64> {
        if (option::is_some(&self.spent_balance)) {
            option::some(balance::value(option::borrow(&self.spent_balance)))
        } else {
            option::none()
        }
    }

    // === Internal: Rule Key ===

    /// Internal: generate a DF Key for an `action` and a `Rule` type.
    fun key<Rule>(): RuleKey { RuleKey { rule: type_name::get<Rule>() } }

    // === Testing ===

    #[test_only]
    public fun new_policy_for_testing<T>(
        ctx: &mut TxContext
    ): (TokenPolicy<T>, TokenPolicyCap<T>) {
        let policy = TokenPolicy {
            id: object::new(ctx),
            rules: vec_map::empty(),
            spent_balance: balance::zero(),
        };
        let cap = TokenPolicyCap {
            id: object::new(ctx),
            for: object::id(&policy)
        };

        (policy, cap)
    }

    #[test_only]
    public fun burn_policy_for_testing<T>(
        policy: TokenPolicy<T>,
        cap: TokenPolicyCap<T>
    ) {
        let TokenPolicyCap { id: cap_id, for: _ } = cap;
        let TokenPolicy { id, rules: _, spent_balance } = policy;
        balance::destroy_for_testing(spent_balance);
        object::delete(cap_id);
        object::delete(id);
    }

    #[test_only]
    public fun mint_for_testing<T>(amount: u64, ctx: &mut TxContext): Token<T> {
        let balance = balance::create_for_testing(amount);
        Token { id: object::new(ctx), balance, owner: tx_context::sender(ctx) }
    }

    #[test_only]
    public fun burn_for_testing<T>(token: Token<T>) {
        let Token { id, balance, owner: _ } = token;
        balance::destroy_for_testing(balance);
        object::delete(id);
    }
}
