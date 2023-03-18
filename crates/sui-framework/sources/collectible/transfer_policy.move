// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Part of the collectibles bundle - a primitive allowing creators to enforce
/// constraints on transfers as long as the transfers are performed in the ecosystem
/// that enforces them.
module sui::transfer_policy {
    use std::vector;
    use std::option::{Self, Option};
    use std::type_name::{Self, TypeName};
    use sui::package::{Self, Publisher};
    use sui::tx_context::TxContext;
    use sui::object::{Self, ID, UID};
    use sui::vec_set::{Self, VecSet};
    use sui::dynamic_field as df;
    use sui::bag::{Self, Bag};
    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::event;

    friend sui::collectible;

    /// The number of receipts does not match the `TransferPolicy` requirement.
    const EPolicyNotSatisfied: u64 = 0;
    /// A completed rule is not set in the `TransferPolicy`.
    const EIllegalRule: u64 = 1;
    /// A Rule is not set.
    const EUnknownRequrement: u64 = 2;
    /// Attempting to create a Rule that is already set.
    const ERuleAlreadySet: u64 = 3;
    /// Trying to `withdraw` or `close_and_withdraw` with a wrong Cap.
    const ENotOwner: u64 = 4;
    /// Trying to `withdraw` more than there is.
    const ENotEnough: u64 = 5;

    /// A "Hot Potato" forcing the buyer to get a transfer permission
    /// from the item type (`T`) owner on purchase attempt.
    struct TransferRequest<phantom T: key + store> {
        /// Amount of SUI paid for the item. Can be used to
        /// calculate the fee / transfer policy enforcement.
        paid: u64,
        /// The ID of the Kiosk / Safe the object is being sold from.
        /// Can be used by the TransferPolicy implementors.
        from: ID,
        /// A Bag of custom details attached to the `TransferRequest`.
        /// The attachments must be resolved before the `TransferRequest`
        /// can be completed and unpacked to accept the transfer.
        metadata: Bag,
        /// Collected Receipts. Used to verify that all of the rules
        /// were followed and `TransferRequest` can be confirmed.
        receipts: VecSet<TypeName>
    }

    /// A unique capability that allows owner of the `T` to authorize
    /// transfers. Can only be created with the `Publisher` object.
    struct TransferPolicy<phantom T: key + store> has key, store {
        id: UID,
        /// The Balance of the `TransferPolicy` which collects `SUI`.
        /// By default, transfer policy does not collect anything , and it's
        /// a matter of an implementation of a specific rule - whether to add
        /// to balance and how much.
        balance: Balance<SUI>,
        /// Set of types of attached rules.
        rules: VecSet<TypeName>
    }

    /// A Capability granting the owner permission to add/remove rules as well
    /// as to `withdraw` and `destroy_and_withdraw` the `TransferPolicy`.
    struct TransferPolicyCap<phantom T: key + store> has key, store {
        id: UID,
        policy_id: ID
    }

    /// Event that is emitted when a publisher creates a new `TransferPolicyCap`
    /// making the discoverability and tracking the supported types easier.
    struct TransferPolicyCreated<phantom T: key + store> has copy, drop { id: ID }

    /// Key to store "Rule" configuration for a specific `TransferPolicy`.
    struct RuleKey<phantom T: drop> has copy, store, drop {}

    /// Construct a new `TransferRequest` hot potato which requires an
    /// approving action from the creator to be destroyed / resolved.
    public fun new_request<T: key + store>(
        paid: u64, from: ID, ctx: &mut TxContext
    ): TransferRequest<T> {
        TransferRequest {
            paid, from, receipts: vec_set::empty(), metadata: bag::new(ctx)
        }
    }

    /// Register a type in the Kiosk system and receive an `TransferPolicyCap`
    /// which is required to confirm kiosk deals for the `T`. If there's no
    /// `TransferPolicyCap` available for use, the type can not be traded in
    /// kiosks.
    public fun new<T: key + store>(
        pub: &Publisher, ctx: &mut TxContext
    ): (TransferPolicy<T>, TransferPolicyCap<T>) {
        assert!(package::from_package<T>(pub), 0);
        let id = object::new(ctx);
        let policy_id = object::uid_to_inner(&id);

        event::emit(TransferPolicyCreated<T> { id: policy_id });

        (
            TransferPolicy { id, rules: vec_set::empty(), balance: balance::zero() },
            TransferPolicyCap { id: object::new(ctx), policy_id }
        )
    }

    /// Special case for the `sui::collectible` module to be able to register a
    /// type without a `Publisher` object. Is not magical and a similar logic
    /// can be implemented for the regular `new_transfer_policy_cap` call for
    /// wrapped types.
    public(friend) fun new_protected<T: key + store>(
        ctx: &mut TxContext
    ): (TransferPolicy<T>, TransferPolicyCap<T>) {
        let id = object::new(ctx);
        let policy_id = object::uid_to_inner(&id);

        event::emit(TransferPolicyCreated<T> { id: policy_id });

        (
            TransferPolicy { id, rules: vec_set::empty(), balance: balance::zero() },
            TransferPolicyCap { id: object::new(ctx), policy_id }
        )
    }

    /// Withdraw some amount of profits from the `TransferPolicy`. If amount is not
    /// specified, all profits are withdrawn.
    public fun withdraw<T: key + store>(
        self: &mut TransferPolicy<T>, cap: &TransferPolicyCap<T>, amount: Option<u64>, ctx: &mut TxContext
    ): Coin<SUI> {
        assert!(object::id(self) == cap.policy_id, ENotOwner);

        let amount = if (option::is_some(&amount)) {
            let amt = option::destroy_some(amount);
            assert!(amt <= balance::value(&self.balance), ENotEnough);
            amt
        } else {
            balance::value(&self.balance)
        };

        coin::take(&mut self.balance, amount, ctx)
    }

    /// Destroy a TransferPolicyCap.
    /// Can be performed by any party as long as they own it.
    public fun destroy_and_withdraw<T: key + store>(
        self: TransferPolicy<T>, cap: TransferPolicyCap<T>, ctx: &mut TxContext
    ): Coin<SUI> {
        assert!(object::id(&self) == cap.policy_id, ENotOwner);

        let TransferPolicyCap { id: cap_id, policy_id: _ } = cap;
        let TransferPolicy { id, rules: _, balance } = self;

        object::delete(id);
        object::delete(cap_id);
        coin::from_balance(balance, ctx)
    }

    /// Allow a `TransferRequest` for the type `T`. The call is protected
    /// by the type constraint, as only the publisher of the `T` can get
    /// `TransferPolicy<T>`.
    ///
    /// Note: unless there's a policy for `T` to allow transfers,
    /// Kiosk trades will not be possible.
    public fun confirm_request<T: key + store>(
        self: &TransferPolicy<T>, request: TransferRequest<T>
    ): (u64, ID) {
        let TransferRequest { paid, from, receipts, metadata } = request;
        let completed = vec_set::into_keys(receipts);
        let total = vector::length(&completed);

        assert!(total == vec_set::size(&self.rules), EPolicyNotSatisfied);

        while (total > 0) {
            let rule_type = vector::pop_back(&mut completed);
            assert!(vec_set::contains(&self.rules, &rule_type), EIllegalRule);
            total = total - 1;
        };

        bag::destroy_empty(metadata);
        (paid, from)
    }

    // === Rules Logic ===

    /// Add a custom Rule to the `TransferPolicy`. Once set, `TransferRequest` must
    /// receive a confirmation of the rule executed so the hot potato can be unpacked.
    ///
    /// - T: the type to which TransferPolicy<T> is applied.
    /// - Rule: the witness type for the Custom rule
    /// - Config: a custom configuration for the rule
    ///
    /// Config requires `drop` to allow creators to remove any policy at any moment,
    /// even if graceful unpacking has not been implemented in a "rule module".
    public fun add_rule<T: key + store, Rule: drop, Config: store + drop>(
        _: Rule, policy: &mut TransferPolicy<T>, cap: &TransferPolicyCap<T>, cfg: Config
    ) {
        assert!(object::id(policy) == cap.policy_id, ENotOwner);
        assert!(!has_rule<T, Rule>(policy), ERuleAlreadySet);
        df::add(&mut policy.id, RuleKey<Rule> {}, cfg);
        vec_set::insert(&mut policy.rules, type_name::get<Rule>())
    }

    /// Get the custom Config for the Rule (can be only one per "Rule" type).
    public fun get_rule<T: key + store, Rule: drop, Config: store + drop>(
        _: Rule, policy: &TransferPolicy<T>)
    : &Config {
        df::borrow(&policy.id, RuleKey<Rule> {})
    }

    /// Add some `SUI` to the balance of a `TransferPolicy`.
    public fun add_to_balance<T: key + store, Rule: drop>(
        _: Rule, policy: &mut TransferPolicy<T>, coin: Coin<SUI>
    ) {
        assert!(has_rule<T, Rule>(policy), EUnknownRequrement);
        coin::put(&mut policy.balance, coin)
    }

    /// Adds a `Receipt` to the `TransferRequest`, unblocking the request and
    /// confirming that the policy requirements are satisfied.
    public fun add_receipt<T: key + store, Rule: drop>(
        _: Rule, request: &mut TransferRequest<T>
    ) {
        vec_set::insert(&mut request.receipts, type_name::get<Rule>())
    }

    /// Check whether a custom rule has been added to the `TransferPolicy`.
    public fun has_rule<T: key + store, Rule: drop>(policy: &TransferPolicy<T>): bool {
        df::exists_(&policy.id, RuleKey<Rule> {})
    }

    /// Remove the Rule from the `TransferPolicy`.
    public fun remove_rule<T: key + store, Rule: drop, Config: store + drop>(
        policy: &mut TransferPolicy<T>, cap: &TransferPolicyCap<T>
    ) {
        assert!(object::id(policy) == cap.policy_id, ENotOwner);
        let _: Config = df::remove(&mut policy.id, RuleKey<Rule> {});
    }

    // === Fields access ===

    /// Get the `paid` field of the `TransferRequest`.
    public fun paid<T: key + store>(self: &TransferRequest<T>): u64 { self.paid }

    /// Get the `from` field of the `TransferRequest`.
    public fun from<T: key + store>(self: &TransferRequest<T>): ID { self.from }

    /// Get the `metadata_mut` field of the `TransferRequest`.
    public fun metadata_mut<T: key + store>(self: &mut TransferRequest<T>): &mut Bag { &mut self.metadata }
}

#[test_only]
/// An example module implementing a fixed commission for the `TransferPolicy`.
/// Follows the "transfer rules" layout and implements each of the steps.
module sui::fixed_commission {
    use sui::sui::SUI;
    use sui::coin::Coin;
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferRequest,
        TransferPolicyCap
    };

    /// Expected amount does not match the passed one.
    const EIncorrectAmount: u64 = 0;

    /// Custom witness-key which also acts as a key for the policy.
    struct Rule has drop {}

    /// Fixed commission on all sales.
    struct Commission has store, drop { amount: u64 }

    /// Creator action: adds a Rule;
    /// Set a FixedCommission requirement for the TransferPolicy.
    public fun set<T: key + store>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>,
        amount: u64
    ) {
        policy::add_rule(Rule {}, policy, cap, Commission { amount });
    }

    /// Creator action: remove the rule from the policy.
    /// Can be performed freely at any time, this method only helps fill-in type params.
    public fun unset<T: key + store>(policy: &mut TransferPolicy<T>, cap: &TransferPolicyCap<T>) {
        policy::remove_rule<T, Rule, Commission>(policy, cap)
    }

    /// Buyer action: perform required action;
    /// Complete the requirement on `TransferRequest`. In this case - pay the fixed fee.
    public fun pay<T: key + store>(
        policy: &mut TransferPolicy<T>, request: &mut TransferRequest<T>, coin: Coin<SUI>
    ) {
        let paid = policy::paid(request);
        let config: &Commission = policy::get_rule(Rule {}, policy);

        assert!(paid == config.amount, EIncorrectAmount);

        policy::add_to_balance(Rule {}, policy, coin);
        policy::add_receipt(Rule {}, request);
    }
}

#[test_only]
module sui::dummy_policy {
    use sui::coin::Coin;
    use sui::sui::SUI;
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    struct Rule has drop {}
    struct Config has store, drop {}

    public fun set<T: key + store>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>
    ) {
        policy::add_rule(Rule {}, policy, cap, Config {})
    }

    public fun pay<T: key + store>(
        policy: &mut TransferPolicy<T>,
        request: &mut TransferRequest<T>,
        payment: Coin<SUI>
    ) {
        policy::add_to_balance(Rule {}, policy, payment);
        policy::add_receipt(Rule {}, request);
    }
}

#[test_only]
module sui::malicious_policy {
    use sui::transfer_policy::{Self as policy, TransferRequest};

    struct Rule has drop {}

    public fun cheat<T: key + store>(request: &mut TransferRequest<T>) {
        policy::add_receipt(Rule {}, request);
    }
}

#[test_only]
module sui::transfer_policy_test {
    use sui::transfer_policy::{Self as policy, TransferPolicy, TransferPolicyCap};
    use sui::tx_context::{TxContext, dummy as ctx};
    use sui::object::{Self, UID};
    use sui::dummy_policy;
    use sui::malicious_policy;
    use sui::package;
    use sui::coin;

    struct OTW has drop {}
    struct Asset has key, store { id: UID }

    #[test]
    /// No policy set;
    fun test_default_flow() {
        let ctx = &mut ctx();
        let (policy, cap) = prepare(ctx);

        // time to make a new transfer request
        let request = policy::new_request(10_000, object::new_id(ctx), ctx);
        policy::confirm_request(&policy, request);

        wrapup(policy, cap, ctx);
    }

    #[test]
    /// Policy set and completed;
    fun test_rule_completed() {
        let ctx = &mut ctx();
        let (policy, cap) = prepare(ctx);

        // now require everyone to pay any amount
        dummy_policy::set(&mut policy, &cap);

        let request = policy::new_request(10_000, object::new_id(ctx), ctx);

        dummy_policy::pay(&mut policy, &mut request, coin::mint_for_testing(10_000, ctx));
        policy::confirm_request(&policy, request);

        let profits = wrapup(policy, cap, ctx);

        assert!(profits == 10_000, 0);
    }

    #[test]
    #[expected_failure(abort_code = sui::transfer_policy::EPolicyNotSatisfied)]
    /// Policy set but not satisfied;
    fun test_rule_ignored() {
        let ctx = &mut ctx();
        let (policy, cap) = prepare(ctx);

        // now require everyone to pay any amount
        dummy_policy::set(&mut policy, &cap);

        let request = policy::new_request(10_000, object::new_id(ctx), ctx);
        policy::confirm_request(&policy, request);

        wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::transfer_policy::ERuleAlreadySet)]
    /// Attempt to add another policy;
    fun test_rule_exists() {
        let ctx = &mut ctx();
        let (policy, cap) = prepare(ctx);

        // now require everyone to pay any amount
        dummy_policy::set(&mut policy, &cap);
        dummy_policy::set(&mut policy, &cap);

        let request = policy::new_request(10_000, object::new_id(ctx), ctx);
        policy::confirm_request(&policy, request);

        wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::transfer_policy::EIllegalRule)]
    /// Attempt to cheat by using another rule approval;
    fun test_rule_swap() {
        let ctx = &mut ctx();
        let (policy, cap) = prepare(ctx);

        // now require everyone to pay any amount
        dummy_policy::set(&mut policy, &cap);
        let request = policy::new_request(10_000, object::new_id(ctx), ctx);

        // try to add receipt from another rule
        malicious_policy::cheat(&mut request);
        policy::confirm_request(&policy, request);

        wrapup(policy, cap, ctx);
    }

    fun prepare(ctx: &mut TxContext): (TransferPolicy<Asset>, TransferPolicyCap<Asset>) {
        let publisher = package::test_claim(OTW {}, ctx);
        let (policy, cap) = policy::new<Asset>(&publisher, ctx);
        package::burn_publisher(publisher);
        (policy, cap)
    }

    fun wrapup(policy: TransferPolicy<Asset>, cap: TransferPolicyCap<Asset>, ctx: &mut TxContext): u64 {
        let profits = policy::destroy_and_withdraw(policy, cap, ctx);
        coin::burn_for_testing(profits)
    }
}
