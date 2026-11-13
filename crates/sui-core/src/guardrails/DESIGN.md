# Guardrails

Today if your private key gets stolen, everything in the
account is gone, and there is nothing the chain can do about it. We can do a lot better:
let an account opt in to a bound on what its key alone can do, so that a stolen key costs you at
most some amount per epoch, and anything beyond that needs a second approver. The rules you set
on your account is what we call a policy in this doc, and the second approver is the guardian.

The first version of this proposal only bounds one thing: how much of each balance
can leave the account per epoch. It does not protect NFTs or any other objects. The target users
are accounts that mostly don't do much: long term holders on hardware wallets, staking addresses,
treasuries where one key does the daily operations and a second person signs off on anything
large.

## The rule

While a policy is active, a transaction that draws on the account's balances either gets refused
at signing, or fails with a new `GuardrailsViolation` status, if it breaks the rule below. A
transaction that the guardian sponsors is exempt (more on that later).

The rule: the net amount of each coin type that leaves the account per epoch, counting coin
objects and address balance together, must stay under that coin type's limit. Gas counts as SUI
outflow. Coins you keep, or send to an allowed recipient, don't count.

Per coin type you configure a per epoch limit and a per transaction cap (we will explain why we
need the per tx cap in the budget section). A coin type without an entry cannot flow out at all,
and the gas budget has to fit the SUI cap. There is also an allowed recipients list, which is
meant for your own other accounts.

To make "leaves the account" precise, we need to define what the policy considers as balance you
hold, and we keep this deliberately narrow: only what your address holds directly, i.e. `Coin<T>`
objects owned by the address (they show up as owned inputs of the transaction), `StakedSui` objects
owned by the address (their principal counts as SUI), and your address balance. Nothing held through
an object counts: not a coin owned by one of your objects, not a coin sitting in a dynamic field,
and not a `Balance<T>` wrapped inside some struct. Those are all outside of the policy, and can be
added later.

Given that, a balance leaves the account when, after the transaction, it is no longer in one of
those forms under your address or an allowed recipient's. That includes the obvious cases, i.e.
transferred to someone else, shared or frozen, or burned. It also includes moving it into an object:
sending a coin to an object, putting it in a dynamic field, or wrapping it all count as outflow even
when the object is yours, because from then on the policy can't see it anymore. So e.g. depositing
into a DeFi position spends your budget.

A few things the policy does not bound, and we are ok with that for v1:

- Non-coin objects and whatever balance is inside them. NFTs, DeFi positions, coins owned by your
  objects and etc. can still be transferred with the key alone, and whatever balance is inside
  them goes with them without touching your budget. Protecting objects needs a package allowlist
  with custody
  rules, that's the next layer.
- What a package does with the value you deposited into it. The deposit already spent your
  budget so the loss is bounded, but from then on it's the package's business.
- Whether a transfer under the limit makes sense.
- Total loss over many epochs. The limit resets every epoch, so you still need to notice at some
  point.
- Supply you can mint. If you hold a `TreasuryCap` or `Supply` you can still mint without bound,
  so such accounts should not rely on this yet.

## Where the policy lives

Every policy is a dynamic field of a `GuardrailsRegistry` system object at `0xab`, keyed by the
owner address. `enable` takes the registry as a mutable input and adds the
field, and later changes borrow the field mutably through the registry.

```move
module sui::guardrails;

use std::type_name::TypeName;
use sui::table::Table;
use sui::vec_map::VecMap;

/// Most coin types one policy can limit. Keeps the field small enough to load whole
/// and bounds the reservations one transaction creates.
const MAX_LIMITED_TYPES: u64 = 32;

/// System object at 0xab, created at genesis. Every transaction reads it
/// implicitly; policies are its dynamic fields.
public struct GuardrailsRegistry has key {
    id: UID,
}

/// Dynamic-field key of an owner's policy.
public struct PolicyKey has copy, drop, store {
    owner: address,
}

public struct Policy has store {
    guardian: address,
    /// First epoch in which the policy is enforced; u64::MAX once cancelled.
    active_from: u64,
    /// First epoch in which the policy is no longer enforced; u64::MAX while active.
    active_until: u64,
    /// Limits in force, keyed by coin type.
    limits: VecMap<TypeName, BalanceLimit>,
    /// Limits that replace `limits` from `next_limits_epoch` on, if set.
    next_limits: Option<VecMap<TypeName, BalanceLimit>>,
    next_limits_epoch: u64,
    /// Addresses that may receive without limit.
    allowed_recipients: Table<address, bool>,
}

public struct BalanceLimit has copy, drop, store {
    /// Most that may leave the account per epoch.
    per_epoch: u64,
    /// Most that may leave in one transaction; also what each transaction reserves.
    per_tx: u64,
}
```

Limits are a `VecMap` instead of a `Table` on purpose. The scheduler, admission and execution all
need every limit of the account at once, and with a `VecMap` that is one field read instead of one
child object read per coin type. That only works if the map stays small, so `set_limits` caps it at
`MAX_LIMITED_TYPES`. The cap also bounds how many reservations a single transaction can put into the
scheduler, which is validator work the sender does not pay for. The recipient list goes the other
way: it is only ever looked up by key at execution, and it can get long, so it is a `Table` and the
field stays small no matter how many recipients there are. `sui_types::guardrails::Policy` mirrors
this layout
for the Rust side.

One property we want to hold: the policy for any given epoch is a pure function of the field, and
no write ever changes the policy of the current epoch. So within an epoch, whatever version of
the field you read, you get the same policy. This is what later allows the scheduler to read it
outside of consensus order.

```move
fun is_active(p: &Policy, e: u64): bool { p.active_from <= e && e < p.active_until }

fun limits_for(p: &Policy, e: u64): &VecMap<TypeName, BalanceLimit> {
    if (p.next_limits.is_some() && p.next_limits_epoch <= e) p.next_limits.borrow() else &p.limits
}

/// Runs first in every mutation, so a pending set whose epoch has arrived is in `limits`
/// before anything overwrites `next_limits`.
fun promote(p: &mut Policy, e: u64) {
    if (p.next_limits.is_some() && p.next_limits_epoch <= e) { p.limits = p.next_limits.extract() }
}

/// Changes to an active policy need the guardian; a pending or inactive one needs only the key.
fun authorized(p: &Policy, ctx: &TxContext): bool {
    !is_active(p, ctx.epoch()) || ctx.sponsor() == option::some(p.guardian)
}

/// Shared prologue of every mutation: the sender's own policy, authorization, promotion.
fun borrow_policy(registry: &mut GuardrailsRegistry, ctx: &TxContext): &mut Policy {
    let p = df::borrow_mut(&mut registry.id, PolicyKey { owner: ctx.sender() });
    assert!(authorized(p, ctx), ENotAuthorized);
    promote(p, ctx.epoch());
    p
}

public fun enable(registry: &mut GuardrailsRegistry, guardian: address, ctx: &mut TxContext) {
    let (owner, e) = (ctx.sender(), ctx.epoch());
    assert!(guardian != owner, EGuardianIsOwner);
    if (!df::exists_(&registry.id, PolicyKey { owner })) {
                // empty limits, no next_limits, a fresh recipients Table
        df::add(&mut registry.id, PolicyKey { owner }, Policy { guardian, active_from: e + 1, active_until: MAX, .. });
        return
    };
    let p = borrow_policy(registry, ctx);
    assert!(!is_active(p, e), EAlreadyActive);
        p.guardian = guardian;
    p.active_from = e + 1;
    p.active_until = MAX;
}

public fun cancel(registry: &mut GuardrailsRegistry, ctx: &TxContext) {
    let p = borrow_policy(registry, ctx);
    assert!(p.active_from > ctx.epoch(), ENotPending);
        p.active_from = MAX;
}

public fun disable(registry: &mut GuardrailsRegistry, ctx: &TxContext) {
    let p = borrow_policy(registry, ctx);
    assert!(is_active(p, ctx.epoch()), ENotActive);
    p.active_until = ctx.epoch() + 1;
}

public fun set_limits(
    registry: &mut GuardrailsRegistry,
    limits: VecMap<TypeName, BalanceLimit>,
    ctx: &TxContext,
) {
    assert!(limits.length() <= MAX_LIMITED_TYPES, ETooManyLimits);
    let e = ctx.epoch();
    let p = borrow_policy(registry, ctx);
    if (is_active(p, e)) {
        p.next_limits = option::some(limits);
        p.next_limits_epoch = e + 1;
    } else {
        p.limits = limits;
    }
}

public fun allow_recipient(registry: &mut GuardrailsRegistry, recipient: address, ctx: &TxContext) {
    borrow_policy(registry, ctx).allowed_recipients.add(recipient, true);
}
// disallow_recipient mirrors it. Both take effect at once.
```

So a policy is pending from `enable` until `active_from`, during which you can configure or
`cancel` it with the key alone and nothing is enforced. Then it is active until `active_until`,
during which every transaction that draws on the account is checked, and any change needs the
guardian (limit changes and `disable` only take effect next epoch). After `cancel` or
`active_until` it is disabled, the field stays, and `enable` starts over. Note that an active
policy cannot be replaced: `enable` aborts, and only the guardian can change the guardian, so a
key alone cannot take the account out of its policy.

Why the one epoch delay: it comes from the funds withdraw scheduler. The per epoch
budget is decided before execution by the funds withdraw scheduler (see Per epoch budgets), and that
scheduler must make its decisions concurrently without waiting on any user transaction to execute,
otherwise we would be serializing a sender's transactions again. So everything it reads has to give
the same answer no matter when it reads it. For the spend counter that holds because it reads
settled values at consensus assigned accumulator versions. For the policy, which it reads as the
latest state in the store, it only holds if the policy does not change during the scheduler's
lifetime, and the scheduler lives for one epoch. Hence `enable`, `disable` and `set_limits` all take
effect at the next epoch. For `enable` there is a second reason: if someone holding your key enables
a policy with their own guardian to lock you out, the pending epoch is when you see it and cancel it
with the key. We thought about whether an attacker gains anything from locking you out instead of
just taking the money, and we don't think so, so there is no further defense needed here.

## How a transaction finds its policies

A transaction never declares the policies it is subject to. Instead, we look at which accounts a
transaction can draw value from, and that is already fixed by the transaction data: the sender (its
coins and address balance), the gas owner (its gas coins and the gas), and the funder of each
allowance withdrawal (the declared amount). A stolen key can act in every one of these roles, and
the gas owner and funder cases matter because there the value leaves in a transaction the owner did
not send:

```rust
/// Up to three accounts: the sender (coins, address balance), the gas owner (gas coins, gas,
/// and declared sponsor withdrawals once `WithdrawFrom::Sponsor` passes validity), and the
/// funder of each allowance withdrawal (the declared amount).
fn accounts_drawn_on(tx: &TransactionData) -> BTreeSet<SuiAddress> {
    [tx.sender(), tx.gas_owner()]
        .into_iter()
        .chain(tx.funds_withdrawals().filter_map(|w| match w.withdraw_from {
            WithdrawFrom::SenderAllowance { funder, .. } => Some(funder),
            _ => None,
        }))
        .collect()
}

/// The registry is pinned at the version consensus assigned; the field read is bounded by it.
fn policy(registry: &PinnedObject, account: SuiAddress) -> Option<Policy> {
    registry.dynamic_field(PolicyKey { owner: account })
}

fn exempt(policy: &Policy, tx: &TransactionData) -> bool {
    tx.gas_owner() == policy.guardian
}
```

The registry is an implicitly read system object, same as the accumulator root and the
forwarding address registry: every transaction reads it at a version assigned by consensus,
without declaring it (`IMPLICITLY_READ_SYSTEM_OBJECTS`, `SystemObjectVersions`). So nothing per
account happens at sequencing. A transaction that changes a policy declares the registry as a
mutable input and takes the next version like any shared object write. Execution loads the fields
through the same child object resolver that the Move runtime uses, and an absent field simply
means no policy.

The funds withdraw scheduler and admission also need to know whether an account has an active policy
and
what its limits are, and they do that outside of consensus order, by reading the latest field
from the store. That is ok precisely because of the property above: `is_active` and `limits_for`
for the current epoch never change within the epoch, so every node gets the same answer no matter
what it has executed so far.

The cost is that every transaction's effects carry the registry version, and every policy change
writes the one registry, so policy changes serialize network wide. They are rare so we are not
worried.

## Guardian approval is just sponsorship

The guardian approves a transaction by
sponsoring it, i.e. the guardian is the gas owner and signs it just like any sponsor does today.
The gas owner is part of `TransactionData`, so the approval is covered by the digest, a copy
without the guardian signature is simply invalid, and admission, scheduler, execution and dry-run
all see the approval from the transaction data (that's the `exempt` function above). Move sees it
through `tx_context::sponsor()` in `authorized`. There is no new signature rule or
verification code to write.

This also means any Sui account can be a guardian, since sponsorship goes through the ordinary
signature verifier: passkey, zkLogin, a plain key, or a multisig combining several of them, which
is how you would compose a stronger second factor. The guardian pays gas for what it approves,
and its gas coins are exposed to the PTB like any sponsor, so a guardian wallet should review
what it signs. A guardian who has a policy themselves is only affected by their own policy's view
of the gas they pay.

The one limitation: a transaction cannot have both a third party gas sponsor and a guardian.
Approved transactions should be rare, so we are fine with it.

## The check at execution

The check runs in the adapter after the PTB finished, next to the other post execution checks in
`check_effects`. A violation fails the transaction like any other execution error, i.e. writes are
dropped and
gas is charged. Sender and gas owner are checked from execution results, because a
sponsored transaction can spend the sponsor's gas coins in the PTB and not just gas. A funder's
exposure is exactly the declared withdraw, so that one needs no execution result.

```rust
for account in accounts_drawn_on(tx) {
    let Some(p) = policy(registry, account) else { continue };
    if !is_active(&p, epoch) || exempt(&p, tx) { continue }
    let caps = limits_for(&p, epoch);           // a type with no entry has per_tx 0
    match role(account, tx) {
        Sender | GasOwner => {
            // Sender: Coin<T> and StakedSui objects owned directly by the address, i.e. owned
            // inputs, at their input versions (StakedSui principal counts as SUI). GasOwner: the
            // gas coins.
            let held_before: Map<T, u64> = coins_held_before(account, tx);
            // The same objects, plus objects created by the transaction, that end owned by the
            // account or by an allowed recipient.
            let kept_after: Map<T, u64> = coins_kept_after(account, &p.allowed_recipients, effects);
            let mut outflow = held_before - kept_after + net_address_balance_withdrawals(account);
            if account == tx.gas_owner() { outflow[SUI] += gas_budget }
            for (ty, amount) in outflow {
                ensure!(amount <= caps[ty].per_tx, GuardrailsViolation { account, kind: OutflowExceedsCap { ty } })
            }
        }
        Funder { ty, declared } => {
            ensure!(declared <= caps[ty].per_tx, GuardrailsViolation { account, kind: OutflowExceedsCap { ty } })
        }
    }
}
// After gas is charged, for every checked account and type: Merge(Spent<ty> of account, actual
// outflow), with gas_used in place of gas_budget. A transaction that failed in Move merges only
// its gas.
```

```rust
ExecutionFailureStatus::GuardrailsViolation { account: SuiAddress, kind: GuardrailsViolationKind }

enum GuardrailsViolationKind {
    /// Outflow of `coin_type` in this transaction exceeds the per-transaction cap.
    OutflowExceedsCap { coin_type: TypeTag },
    /// The scheduler found no room left in the epoch budget of `coin_type`; the transaction
    /// did not execute and paid no gas.
    BudgetExhausted { coin_type: TypeTag },
}
```

Note the per epoch limit is not checked here at all. That is the scheduler's job, next.

## Per epoch budgets

Limits have to bound an epoch, not a transaction, otherwise you just repeat the transaction. The
question is how to track the spend deterministically without serializing the owner's
transactions, and the answer we landed on is to do exactly what address balance withdraws do:
decide before execution, in consensus order, from a declared upper bound, and execution only has
to stay within the bound. The owner's transactions keep running in parallel, and every node
reaches the same decision without reading anything another transaction wrote in the same commit.

Counters. The spend lives in accumulator fields `guardrails::Spent<T>`, one per owner and coin
type, under the accumulator root like a balance. Execution can only merge into a counter, it can
never read or reset it. So the reset has to happen where the field is actually written, which is
settlement. This is the same trick the allowance module does inside its own object when a spend
lands in a new window, just moved to the one place that writes an accumulator.

```move
/// Value of a `Spent<T>` field. Settlement resets it when the epoch moves on.
public struct EpochU128 has store { epoch: u64, value: u128 }
```

```rust
// In the native settlement executor (#28511), for each EpochCounter update:
let mut field = read(counter_id).unwrap_or(EpochU128 { epoch: 0, value: 0 });
if field.epoch < settlement.epoch { field = EpochU128 { epoch: settlement.epoch, value: 0 } }
field.value += update.amount;        // merge only; never aborts
write(counter_id, field);

// What every reader (scheduler, admission, wallets) takes as the spend of `epoch`:
fn spent(field: Option<EpochU128>, epoch: u64) -> u128 {
    match field { Some(f) if f.epoch == epoch => f.value, _ => 0 }
}
```

Every settlement of an epoch finishes before the epoch changes, so the first settlement of the
next epoch sees the old tag and starts from zero. There are no per epoch fields to delete, and
nothing reads a historical version (which we did not want to depend on because of pruning).

This depends on native accumulator settlement (#28511), where it becomes one more update group:

```rust
enum AccumulatorUpdateGroup {
    Balance { coin_type: TypeTag, updates: Vec<BalanceUpdate> },
    EventStream(Vec<EventStreamUpdate>),
    EpochCounter { counter_type: TypeTag, updates: Vec<EpochCounterUpdate> },   // new
}

struct EpochCounterUpdate { owner: SuiAddress, amount: u128 }   // merge only
```

The variant is an on-wire addition under the two-PR rule, on its own schedule. Guardrails ship
after native settlement anyway.

Reservation. The funds withdraw scheduler gets, per transaction, one reservation per policy
bound account and limited coin type, next to any address balance withdraws the transaction
declares:

```rust
fn reservations(tx: &TransactionData, epoch: u64) -> BTreeMap<AccumulatorObjId, u64> {
    let mut out = BTreeMap::new();
    for account in accounts_drawn_on(tx) {
        let Some(p) = latest_policy(account) else { continue };
        if !is_active(&p, epoch) || exempt(&p, tx) { continue }
        let caps = limits_for(&p, epoch);
        match role(account, tx) {
            Sender => for (ty, lim) in caps { out.add(counter(account, ty), lim.per_tx) },
            GasOwner => out.add(counter(account, SUI), caps[SUI].per_tx),   // the PTB can take the gas coins
            Funder { ty, declared } => out.add(counter(account, ty), declared),  // exact
        }
    }
    out
}
```

The scheduler then treats a counter as a balance whose starting value is
`per_epoch − spent(field, epoch)`, with settlement deltas negated. Everything else (reserved
amounts per accumulator version, pending queue, head of line order) is unchanged.

Decision. A transaction fits if for every counter it reserves,
`reservation + already_reserved <= per_epoch − spent`. If it doesn't fit at a settled version it
gets cancelled, through the existing early error exit for insufficient funds, with
`BudgetExhausted` as status: no execution, no writes, no gas charged. So the counters never
exceed the limit, not even by the gas of a refused transaction. Fullnodes don't run the scheduler
and take the decision from the effects status, same as address balances.

Cost. The sender's reservation is the cap rather than the actual spend, and it holds until the
transaction's accumulator version settles, i.e. a checkpoint later. Within that window the sender
can have at most remaining ÷ cap transactions in flight for the tightest coin type, and a
transaction reserves every limited type whether it touches it or not. For the users we are targeting
this does not matter. We could let a transaction declare the types it touches later if
it does.

## Admission

Validators refuse to sign a transaction that cannot pass its policies, so the rejection costs no
gas. This check runs with the other signing checks, on latest state, not a sequenced version:

```rust
for (counter, amount) in reservations(tx, epoch) {
    let (account, ty) = counter.key();
    let caps = limits_for(&latest_policy(account), epoch);
    // The gas budget must fit the gas owner's SUI cap; a declared withdrawal must fit its funder's cap.
    ensure!(declared_amount(tx, account, ty) <= caps[ty].per_tx);
    ensure!(amount <= caps[ty].per_epoch - spent(latest_field(counter), epoch));
}
```

Admission does not track what it already signed, same as address balances. A burst of
transactions signed before any of them settles can all pass admission and exceed the budget, but
then the scheduler cancels the excess for free. The sender's outflow itself needs execution
results, so that is left to execution.

## Effects, replay, dry-run

Every transaction's effects carry the registry version, as they carry the accumulator root's.
The spend shows up as accumulator events on the transaction and settles into the `Spent<T>`
fields. Checkpoint execution, crash recovery and replay already handle implicitly read system
objects. Dry-run and dev-inspect read those at latest versions, so the policy check runs there
against the current registry too, and a guardian sponsored dry-run is exempt just like the real
thing.

The new execution status and the `EpochCounter` settlement group are on-wire changes and go
through the two-PR rule; the feature flag stays devnet only until then.

## Things we considered and rejected

1. A package allowlist with custody rules, plus a rule that non-coin objects stay with the owner.
   Balances don't need it: whatever a package does with a coin, the value either stays with you
   or counts as outflow. Objects do need it, and it brings module level entries for the
   framework, per command attribution of which package took which object, special cases for
   staking and etc. That's the next layer.
2. Treating the sender as the only account a transaction spends from. A stolen key can also
   sponsor someone else's transaction and grant allowances that others withdraw from. Both move
   value in transactions you did not send.
3. One policy object per owner with a derived ID, taken as a per account implicit input. Needs
   per account version assignment at sequencing, a memo of which accounts have active policies,
   and some way around the missing initial shared version in the creation epoch. One registry
   with dynamic fields reuses the implicitly read system object path as is.
4. Guardian approval as an extra signature in the envelope. The digest only covers
   `TransactionData`, so anyone seeing the envelope can resubmit without the guardian signature;
   that copy gets sequenced, fails, and burns the digest. Admission, scheduler and dry-run can't
   see the approval either. Fixing it means declaring the co-signer in `TransactionData`, i.e. a
   new transaction format, and sponsorship already declares the second signer.
5. Guardian approval as a signature passed as PTB data. The signature can't cover a digest that
   contains itself, so wallets need a custom signing format, and on-chain verification is
   limited to Move natives: no zkLogin, no multisig, and passkeys need WebAuthn parsing in Move.
6. Guardian approval as a separate transaction that records an approved digest in the policy.
   Two transactions, digests go stale when the gas coin moves, a list that needs expiry and
   pruning, and a guardian with their own policy gets blocked by it.
7. A per transaction limit instead of per epoch. Not a bound at all. The per tx cap only exists
   to give the scheduler something to reserve.
8. Counters read at execution as the settled value plus the unsettled merges from the same
   commit. Only sound when the writer's transactions are serialized by a shared input. A sender's
   transactions have no shared input, so two in one commit could run in parallel and in different
   orders on different validators: overspend and a fork.
9. The policy object as an implicit mutable input with the spend stored inside. Serializes the
   owner's transactions, introduces a kind of object that gets written without being declared,
   writes an object per transaction, and can't record the gas of a failed transaction on chain.
10. One counter field per owner, epoch and type. Nothing ever deletes the old epochs' fields.
11. A lifetime counter with the epoch's spend taken against its value at the epoch's starting
    accumulator version. A historical read that pruning can remove mid epoch.
12. A lifetime counter capped at `limit × epochs since activation`. No reset at all, but unspent
    budget rolls over, so an account that rarely transacts (i.e. exactly our target user)
    accumulates budget that a stolen key drains at once.
13. Reservations declared in the transaction instead of the per tx cap. More precise, but it
    changes `TransactionData` and every wallet has to compute them.

Thoughts?
