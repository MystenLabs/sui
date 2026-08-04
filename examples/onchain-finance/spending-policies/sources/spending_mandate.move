// Copyright (c) Mysten Labs, Inc.
 // SPDX-License-Identifier: Apache-2.0

 // docs::#spending_mandate
 module example::spending_mandate;

 use sui::balance::{Self, Balance};
 use sui::clock::Clock;
 use sui::coin::Coin;
 use sui::event;

 #[error(code = 0)]
 const EExceedsPerTxLimit: vector<u8> = b"Payment exceeds the per-transaction limit";
 #[error(code = 1)]
 const EExceedsTotalCap: vector<u8> = b"Payment exceeds the remaining mandate balance";
 #[error(code = 2)]
 const ERecipientNotAllowed: vector<u8> = b"Recipient is not allowed";
 #[error(code = 3)]
 const EMandateExpired: vector<u8> = b"Mandate has expired";
 #[error(code = 4)]
 const EWrongOwnerCap: vector<u8> = b"Capability does not match this mandate";
 #[error(code = 5)]
 const ENotAgent: vector<u8> = b"Only the delegated agent can spend";
 #[error(code = 6)]
 const ECapExceedsAvailableFunds: vector<u8> =
     b"Cap exceeds spent plus remaining deposited funds";
 #[error(code = 7)]
 const ENoFundsDeposited: vector<u8> = b"Mandate must be funded";

 public struct MandateCreated<phantom T> has copy, drop {
     mandate_id: ID,
     owner: address,
     agent: address,
     deposited: u64,
     expires_at_ms: u64,
 }

 public struct SpendExecuted<phantom T> has copy, drop {
     mandate_id: ID,
     agent: address,
     recipient: address,
     amount: u64,
     remaining_cap: u64,
 }

 public struct MandateRevoked<phantom T> has copy, drop {
     mandate_id: ID,
     owner: address,
     refunded: u64,
 }

 public struct MandateOwnerCap<phantom T> has key, store {
     id: UID,
     mandate_id: ID,
 }

 /// The mandate owns all delegated funds; the agent cannot transfer them outside this module.
 public struct SpendingMandate<phantom T> has key, store {
     id: UID,
     owner: address,
     agent: address,
     max_per_tx: u64,
     total_cap: u64,
     spent: u64,
     allowed_recipients: vector<address>,
     expires_at_ms: u64,
     funds: Balance<T>,
 }

 /// Deposit delegated funds from a coin and transfer the policy-controlled vault to the agent.
 public fun create_mandate_from_coin<T>(
     coin: Coin<T>,
     agent: address,
     max_per_tx: u64,
     allowed_recipients: vector<address>,
     expires_at_ms: u64,
     ctx: &mut TxContext,
 ): MandateOwnerCap<T> {
     create_mandate(
         coin.into_balance(),
         agent,
         max_per_tx,
         allowed_recipients,
         expires_at_ms,
         ctx,
     )
 }

 /// Deposit delegated funds and transfer the policy-controlled vault to the agent.
 public fun create_mandate<T>(
     funds: Balance<T>,
     agent: address,
     max_per_tx: u64,
     allowed_recipients: vector<address>,
     expires_at_ms: u64,
     ctx: &mut TxContext,
 ): MandateOwnerCap<T> {
     let owner = ctx.sender();
     let total_cap = funds.value();
     assert!(total_cap > 0, ENoFundsDeposited);

     let mandate = SpendingMandate {
         id: object::new(ctx),
         owner,
         agent,
         max_per_tx,
         total_cap,
         spent: 0,
         allowed_recipients,
         expires_at_ms,
         funds,
     };
     let mandate_id = object::id(&mandate);

     event::emit(MandateCreated<T> {
         mandate_id,
         owner,
         agent,
         deposited: total_cap,
         expires_at_ms,
     });

     transfer::transfer(mandate, agent);

     MandateOwnerCap<T> {
         id: object::new(ctx),
         mandate_id,
     }
 }

 /// Send funds held by the mandate after enforcing every delegated-spending constraint.
 public fun execute_spend<T>(
     mandate: &mut SpendingMandate<T>,
     amount: u64,
     recipient: address,
     clock: &Clock,
     ctx: &TxContext,
 ) {
     assert!(ctx.sender() == mandate.agent, ENotAgent);[5:17 PM]assert!(clock.timestamp_ms() < mandate.expires_at_ms, EMandateExpired);
     assert!(amount <= mandate.max_per_tx, EExceedsPerTxLimit);
     assert!(amount <= remaining_cap(mandate), EExceedsTotalCap);
     assert!(mandate.allowed_recipients.contains(&recipient), ERecipientNotAllowed);

     mandate.spent = mandate.spent + amount;

     balance::send_funds(mandate.funds.split(amount), recipient);

     event::emit(SpendExecuted<T> {
         mandate_id: object::id(mandate),
         agent: ctx.sender(),
         recipient,
         amount,
         remaining_cap: remaining_cap(mandate),
     });
 }

 /// Destroy the mandate and refund all unspent custody to the owner.
 /// Because the mandate is agent-owned, the agent must cooperate in the revocation PTB.
 public fun revoke_mandate<T>(cap: MandateOwnerCap<T>, mandate: SpendingMandate<T>) {
     assert!(cap.mandate_id == object::id(&mandate), EWrongOwnerCap);

     let mandate_id = object::id(&mandate);

     let MandateOwnerCap { id: cap_id, .. } = cap;
     cap_id.delete();

     let SpendingMandate { id, owner, funds, .. } = mandate;
     let refunded = funds.value();

     balance::send_funds(funds, owner);

     event::emit(MandateRevoked<T> {
         mandate_id,
         owner,
         refunded,
     });

     id.delete();
 }

 /// Update the total spending cap without changing custody.
 /// The cap cannot go below already-spent funds or above spent plus remaining deposited funds.
 public fun update_cap<T>(
     cap: &MandateOwnerCap<T>,
     mandate: &mut SpendingMandate<T>,
     new_total_cap: u64,
 ) {
     assert!(cap.mandate_id == object::id(mandate), EWrongOwnerCap);
     assert!(new_total_cap >= mandate.spent, EExceedsTotalCap);
     assert!(
         new_total_cap <= mandate.spent + mandate.funds.value(),
         ECapExceedsAvailableFunds,
     );

     mandate.total_cap = new_total_cap;
 }

 public fun add_recipient<T>(
     cap: &MandateOwnerCap<T>,
     mandate: &mut SpendingMandate<T>,
     recipient: address,
 ) {
     assert!(cap.mandate_id == object::id(mandate), EWrongOwnerCap);

     if (!mandate.allowed_recipients.contains(&recipient)) {
         mandate.allowed_recipients.push_back(recipient);
     };
 }

 public fun remaining_cap<T>(mandate: &SpendingMandate<T>): u64 {
     mandate.total_cap - mandate.spent
 }

 public fun is_expired<T>(mandate: &SpendingMandate<T>, clock: &Clock): bool {
     clock.timestamp_ms() >= mandate.expires_at_ms
 }
 // docs::/#spending_mandate
