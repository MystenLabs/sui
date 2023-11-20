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
    /// The number of approvals does not match the number of rules.
    const ESizeMismatch: u64 = 1;
    /// The rule was not approved.
    const ENotApproved: u64 = 2;
    /// Trying to perform an admin action with a wrong cap.
    const ENotAuthorized: u64 = 3;
    /// The balance is too low to perform the action.
    const EBalanceTooLow: u64 = 4;
    /// The balance is not zero.
    const ENotZero: u64 = 5;
    /// The balance is not zero when trying to confirm with `TransferPolicyCap`.
    const ECantConsumeBalance: u64 = 6;
    /// The rule config was not found (on read or get_mut).
    const ERuleConfigNotFound: u64 = 7;

    /// A Tag for the `spend` action.
    const SPEND: vector<u8> = b"spend";
    /// A Tag for the `transfer` action.
    const TRANSFER: vector<u8> = b"transfer";
    /// A Tag for the `to_coin` action.
    const TO_COIN: vector<u8> = b"to_coin";
    /// A Tag for the `from_coin` action.
    const FROM_COIN: vector<u8> = b"from_coin";

    /// A token with closed-loop restrictions set by the issuer
    struct Token<phantom T> has key { id: UID, balance: Balance<T> }

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
        /// The balance that is effectively burned by the user on the "spend"
        /// action. However, actual decrease of the supply can only be done by
        /// the `TreasuryCap` owner.
        ///
        /// This balance can never be withdrawn by anyone and can only be
        /// `flush`-ed by the Admin.
        burned_balance: Balance<T>,
        /// The set of rules that define what actions can be performed on the
        /// token. Each rule contains the set of `TypeName`s that must be
        /// received by the `ActionRequest` for the action to be performed.
        rules: VecMap<String, VecSet<TypeName>>
    }

    /// A request to perform an "Action" on a token. Stores the information
    /// about the performed action and must be consumed by the `confirm_request`
    /// function when the Rules are satisfied.
    struct ActionRequest<phantom T> {
        /// Name of the Action to look up in the Policy.
        ///
        /// > String will always be shorter than the fully qualfied name of the
        /// type (just in case the gas micro optimizations matter).
        name: String,
        /// Amount is present in all of the txs
        amount: u64,
        /// Sender is a permanent field always
        sender: address,
        /// Recipient is only available in `transfer` action.
        recipient: Option<address>,
        /// The balance to be "burned" in the `TokenPolicy`, only available
        /// in the `spend` action.
        burned_balance: Option<Balance<T>>,
        /// Collected approvals (stamps) from completed `Rules`.
        approvals: VecSet<TypeName>,
    }

    /// Dynamic field key for the `TokenPolicy` to store the `Config` for a
    /// specific `Rule` for an action.
    struct RuleKey has store, copy, drop { action: String, rule: TypeName }

    /// Create a new `TokenPolicy` and a matching `TokenPolicyCap`.
    /// The `TokenPolicy` must then be shared using the `share_policy` method.
    public fun new<T>(
        _treasury_cap: &mut TreasuryCap<T>, ctx: &mut TxContext
    ): (TokenPolicy<T>, TokenPolicyCap<T>) {
        let policy = TokenPolicy {
            id: object::new(ctx),
            burned_balance: balance::zero(),
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
    public fun transfer<T>(
        t: Token<T>, recipient: address, ctx: &TxContext
    ): ActionRequest<T> {
        let amount = balance::value(&t.balance);
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
    public fun spend<T>(t: Token<T>, ctx: &TxContext): ActionRequest<T> {
        let Token { id, balance } = t;
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
    public fun to_coin<T>(
        t: Token<T>, ctx: &mut TxContext
    ): (Coin<T>, ActionRequest<T>) {
        let Token { id, balance } = t;
        let amount = balance::value(&balance);
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
        let token = Token { id: object::new(ctx), balance };

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
        let Token { id, balance } = another;
        balance::join(&mut token.balance, balance);
        object::delete(id);
    }

    /// Split a `Token` into two, always publicly available.
    public fun split<T>(
        token: &mut Token<T>, amount: u64, ctx: &mut TxContext
    ): Token<T> {
        assert!(balance::value(&token.balance) >= amount, EBalanceTooLow);
        let balance = balance::split(&mut token.balance, amount);
        Token { id: object::new(ctx), balance }
    }

    /// Create a zero `Token`.
    public fun zero<T>(ctx: &mut TxContext): Token<T> {
        Token { id: object::new(ctx), balance: balance::zero() }
    }

    /// Destroy an empty `Token`, fails if the balance is non-zero.
    public fun destroy_zero<T>(token: Token<T>) {
        let Token { id, balance } = token;
        assert!(balance::value(&balance) == 0, ENotZero);
        balance::destroy_zero(balance);
        object::delete(id);
    }

    /// Transfer the `Token` to the transaction sender.
    public fun keep<T>(token: Token<T>, ctx: &TxContext) {
        transfer::transfer(token, tx_context::sender(ctx))
    }

    // === Request Handling ===

    /// Create a new request to be confirmed by the `TokenPolicy`.
    public fun new_request<T>(
        name: String,
        amount: u64,
        recipient: Option<address>,
        burned_balance: Option<Balance<T>>,
        ctx: &TxContext
    ): ActionRequest<T> {
        ActionRequest {
            name,
            amount,
            recipient,
            burned_balance,
            sender: tx_context::sender(ctx),
            approvals: vec_set::empty(),
        }
    }

    /// Confirm the request against the `TokenPolicy` and return the parameters
    /// of the request: (Name, Amount, Sender, Recipient).
    public fun confirm_request<T>(
        policy: &mut TokenPolicy<T>,
        request: ActionRequest<T>,
        _ctx: &mut TxContext
    ): (String, u64, address, Option<address>) {
        assert!(vec_map::contains(&policy.rules, &request.name), EUnknownAction);

        let ActionRequest {
            name, approvals,
            burned_balance,
            amount, sender, recipient,
        } = request;

        let rules = &vec_set::into_keys(*vec_map::get(&policy.rules, &name));
        let rules_len = vector::length(rules);
        let i = 0;

        assert!(vec_set::size(&approvals) == rules_len, ESizeMismatch);

        while (i < rules_len) {
            let rule = vector::borrow(rules, i);
            assert!(vec_set::contains(&approvals, rule), ENotApproved);
            i = i + 1;
        };

        if (option::is_some(&burned_balance)) {
            balance::join(
                &mut policy.burned_balance,
                option::destroy_some(burned_balance)
            );
        } else {
            option::destroy_none(burned_balance);
        };

        (name, amount, sender, recipient)
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
            burned_balance
        } = request;

        if (option::is_some(&burned_balance)) {
            balance::decrease_supply(
                coin::supply_mut(treasury_cap),
                option::destroy_some(burned_balance)
            );
        } else {
            option::destroy_none(burned_balance);
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
        assert!(option::is_none(&request.burned_balance), ECantConsumeBalance);

        let ActionRequest {
            name, amount, sender, recipient, approvals: _, burned_balance
        } = request;

        option::destroy_none(burned_balance);

        (name, amount, sender, recipient)
    }

    /// Add an approval to the request. Requires a Rule Witness.
    public fun add_approval<T, W: drop>(
        _t: W, request: &mut ActionRequest<T>, _ctx: &mut TxContext
    ) {
        vec_set::insert(&mut request.approvals, type_name::get<W>())
    }

    // === Protected: Setting Rules ===

    /// Allows an action to be performed on the `Token` freely.
    public fun allow<T>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        name: String,
        _ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        vec_map::insert(&mut self.rules, name, vec_set::empty());
    }

    /// Completely disallows an action on the `Token`.
    public fun disallow<T>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        name: String,
        _ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        vec_map::remove(&mut self.rules, &name);
    }

    /// Adds a rule for an action with `name` in the `TokenPolicy`.
    public fun add_rule_for_action<T, Rule: drop, Config: store>(
        _rule: Rule,
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        // should we allow optional config? do we see cases where config is not
        // needed? I kinda do but I'm not sure what's the benefit of it... yet
        config: Config,
        ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        if (!vec_map::contains(&self.rules, &action)) {
            allow(self, cap, action, ctx);
        };

        add_rule<T, Rule, Config>(self, action, config)
    }

    /// Disables a Rule for an action with `name` in the `TokenPolicy`. However,
    /// keeps the Config, as the policy owner may not be able to handle Config
    /// object.
    public fun disable_rule_for_action<T, Rule: drop>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        _ctx: &mut TxContext
    ) {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        disable_rule<T, Rule>(self, action)
    }

    /// Removes a rule for an action with `name` in the `TokenPolicy`. Returns
    /// the config object to be handled by the sender (or a Rule itself).
    public fun remove_rule_for_action<T, Rule: drop, Config: store>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        _ctx: &mut TxContext
    ): Config {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        remove_rule<T, Rule, Config>(self, action)
    }

    /// Allow Rule to mutate the Config object.
    /// We should not allow the Rule module mutate it... it's a security risk.
    public fun get_rule_for_action_mut<T, Rule: drop, Config: store>(
        self: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        _ctx: &mut TxContext
    ): &mut Config {
        assert!(object::id(self) == cap.for, ENotAuthorized);
        let rule_key = key<Rule>(action);
        let exists_ = df::exists_with_type<RuleKey, Config>(&self.id, rule_key);

        assert!(exists_, ERuleConfigNotFound);

        get_rule_mut<T, Rule, Config>(self, action)
    }

    // === Rules API ===

    /// Allow Rule to read the Config object.
    public fun get_rule<T, Rule: drop, Config: store>(
        _rule: Rule,
        self: &TokenPolicy<T>,
        action: String
    ): &Config {
        let rule_key = key<Rule>(action);
        let exists_ = df::exists_with_type<RuleKey, Config>(&self.id, rule_key);

        assert!(exists_, ERuleConfigNotFound);

        df::borrow(&self.id, rule_key)
    }

    // === Protected: Minting and Burning ===

    /// Mint a `Token` with a given `amount` using the `TreasuryCap`.
    public fun mint<T>(
        cap: &mut TreasuryCap<T>, amount: u64, ctx: &mut TxContext
    ): Token<T> {
        let balance = balance::increase_supply(coin::supply_mut(cap), amount);
        Token { id: object::new(ctx), balance }
    }

    /// Burn a `Token` using the `TreasuryCap`.
    public fun burn<T>(
        cap: &mut TreasuryCap<T>, token: Token<T>
    ) {
        let Token { id, balance } = token;
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
        let amount = balance::value(&self.burned_balance);
        let balance = balance::split(&mut self.burned_balance, amount);
        balance::decrease_supply(coin::supply_mut(cap), balance)
    }

    /// Every `TokenPolicy` must be shared in the end.
    public fun share_policy<T>(policy: TokenPolicy<T>) {
        transfer::share_object(policy)
    }

    // === Public Getters ===

    /// Returns the rules required for a specific action.
    public fun rules<T>(self: &TokenPolicy<T>, name: String): VecSet<TypeName> {
        *vec_map::get(&self.rules, &name)
    }

    /// Returns the `burned_balance` of the `TokenPolicy`.
    public fun burned_balance<T>(self: &TokenPolicy<T>): u64 {
        balance::value(&self.burned_balance)
    }

    /// Returns the `balance` of the `Token`.
    public fun value<T>(t: &Token<T>): u64 {
        balance::value(&t.balance)
    }

    // === Action Names ===

    /// Name of the Transfer action.
    public fun transfer_name(): String { string::utf8(TRANSFER) }

    /// Name of the `Spend` action.
    public fun spend_name(): String { string::utf8(SPEND) }

    /// Name of the `ToCoin` action.
    public fun to_coin_name(): String { string::utf8(TO_COIN) }

    /// Name of the `FromCoin` action.
    public fun from_coin_name(): String { string::utf8(FROM_COIN) }

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

    // === Internal: Rule Storage ===

    fun add_rule<T, Rule, Config: store>(
        self: &mut TokenPolicy<T>, action: String, config: Config
    ) {
        let rule = type_name::get<Rule>();
        vec_set::insert(vec_map::get_mut(&mut self.rules, &action), rule);
        df::add(&mut self.id, key<Rule>(action), config);
    }

    fun disable_rule<T, Rule>(self: &mut TokenPolicy<T>, action: String) {
        vec_set::remove(
            vec_map::get_mut(&mut self.rules, &action),
            &type_name::get<Rule>()
        );
    }

    fun remove_rule<T, Rule, Config: store>(
        self: &mut TokenPolicy<T>, action: String
    ): Config {
        let rule = type_name::get<Rule>();
        let rules = vec_map::get_mut(&mut self.rules, &action);
        if (vec_set::contains(rules, &rule)) {
            vec_set::remove(rules, &rule);
        };
        df::remove(&mut self.id, key<Rule>(action))
    }

    fun get_rule_mut<T, Rule: drop, Config: store>(
        self: &mut TokenPolicy<T>, action: String
    ): &mut Config {
        df::borrow_mut(&mut self.id, key<Rule>(action))
    }

    /// Internal: generate a DF Key for an `action` and a `Rule` type.
    fun key<Rule>(action: String): RuleKey {
        RuleKey { action, rule: type_name::get<Rule>() }
    }

    // === Testing ===

    #[test_only]
    public fun new_policy_for_testing<T>(
        ctx: &mut TxContext
    ): (TokenPolicy<T>, TokenPolicyCap<T>) {
        let policy = TokenPolicy {
            id: object::new(ctx),
            rules: vec_map::empty(),
            burned_balance: balance::zero(),
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
        let TokenPolicy { id, rules: _, burned_balance } = policy;
        balance::destroy_for_testing(burned_balance);
        object::delete(cap_id);
        object::delete(id);
    }

    #[test_only]
    public fun mint_for_testing<T>(amount: u64, ctx: &mut TxContext): Token<T> {
        let balance = balance::create_for_testing(amount);
        Token { id: object::new(ctx), balance }
    }

    #[test_only]
    public fun burn_for_testing<T>(token: Token<T>) {
        let Token { id, balance } = token;
        balance::destroy_for_testing(balance);
        object::delete(id);
    }
}
