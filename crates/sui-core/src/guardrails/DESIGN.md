# Guardrails

Today if your private key gets stolen, everything in the account is gone, and there is nothing the
chain can do about it. We can do a lot better: let an account opt in to a bound on what its key
alone can do, so that a stolen key costs you at most some amount per epoch, and anything beyond that
needs a second approver. The rules you set on your account is what we call a policy in this doc, and
the second approver is the guardian.

This doc describes the first version, and we keep it as small as we can so it can ship: one rule,
one account per transaction, and the simplest definition of "your money" that is still useful. The
target users are accounts that mostly don't do much: long term holders on hardware wallets and
custody setups where one key does the daily operations and a second party signs off on anything
large. What we leave out, and how it comes back later without changing the model, is in its own
section below.

## The rule

While a policy is active, a transaction the account sends fails with a new `GuardrailsViolation`
status if it breaks the rule below. A transaction that the guardian sponsors is exempt (more on that
later).

The rule: per coin type and per epoch, the amount that leaves the account's coins and address
balance must stay under that coin type's limit. A coin type without a limit cannot leave at all.

"The account's coins and address balance" means exactly two things:

- `Coin<T>` objects owned by the address alone, either as an address owned or as a consensus
  address owned object, and
- the address balance of `T`.

A coin with several party owners has no single account to count it against, so a transaction
from an account with an active policy cannot take one as an input.

What a transaction takes out is the net decrease of the two together:

```rust
before[T] = value of the sender's Coin<T> inputs + what the transaction withdrew from the sender's address balance
after[T]  = value of the Coin<T> objects the sender owns after + what the transaction deposited into the sender's address balance
outflow[T] = before[T].saturating_sub(after[T])
```

So:

- Splitting and merging your coins, or moving SUI between a coin and your own address balance, is
  not outflow.
- Sending to anyone else is outflow, whether it leaves from a coin or from the address balance.
- Putting value into any other object is outflow too, even an object you keep: a stake, a DeFi
  position, a kiosk. If it doesn't fit the limit, the guardian sponsors it.
- Value that comes back to you in the same transaction offsets what left, e.g. a swap that returns
  the other coin type still spends the type you paid with, but a flash loan you repay is zero.

This definition needs nothing but the transaction's inputs, its written objects and its accumulator
events. No layout walks, no ownership chains.

Things the policy does not bound in this version:

- Gas. A stolen key can burn the account's SUI on gas. The attacker gets nothing out of it, so we
  accept it for now.
- Value that is not in a coin or the address balance: staked SUI, balances inside your own objects,
  NFTs. A stolen key can unstake and send the SUI away in one transaction, and the policy does not
  see it, because the account's coins did not decrease. This is the biggest gap of this version.
- Total loss over many epochs. The limit resets every epoch, so you still need to notice at some
  point.

## Where the policy lives

Every account gets its own policy object. It is a shared object whose ID is derived from the owner
address, `derive_object_id(0xab, PolicyKey(owner))`, so anyone can compute it without a lookup. The
`GuardrailsRegistry` system object at `0xab` is only the derivation parent: `enable` takes it
mutably once to claim the derived UID, and nothing touches it afterwards.

```move
module sui::guardrails;

/// Most coin types one policy can limit. Keeps the object small, since every transaction of the
/// account rewrites it.
const MAX_LIMITED_TYPES: u64 = 32;
const MIN_RECOVERY_DELAY_EPOCHS: u64 = 14;
const MAX_RECOVERY_DELAY_EPOCHS: u64 = 365;
const NEVER: u64 = 0xFFFF_FFFF_FFFF_FFFF;

/// System object at 0xab, created at genesis. Only the parent that policy IDs derive from.
public struct GuardrailsRegistry has key { id: UID }

public struct PolicyKey(address) has copy, drop, store;

/// One per account, shared.
public struct Policy has key {
    id: UID,
    owner: address,
    guardian: address,
    /// First epoch in which the policy is enforced. NEVER once cancelled.
    active_from: u64,
    /// First epoch in which the policy is no longer enforced. NEVER while no end is set.
    active_until: u64,
    /// Most of each coin type that may leave the account per epoch.
    limits: VecMap<TypeName, u64>,
    /// Epochs between `request_disable` and the policy becoming inactive.
    recovery_delay_epochs: u64,
    /// The epoch `spent` belongs to. Written by execution, never by this module.
    spent_epoch: u64,
    spent: VecMap<TypeName, u64>,
}
```

`limits` and `spent` are `VecMap`s rather than `Table`s on purpose: execution needs every entry at
once, and with a `VecMap` that is part of the one object read it already does. That only works if
the maps stay small, hence `MAX_LIMITED_TYPES`. `sui_types::guardrails::Policy` mirrors the layout
for the Rust side.

```move
fun is_active(p: &Policy, e: u64): bool { p.active_from <= e && e < p.active_until }

/// Every change comes from the owner, and a change to an active policy needs the guardian as the
/// transaction's sponsor.
fun assert_authorized(p: &Policy, ctx: &TxContext) {
    assert!(p.owner == ctx.sender(), ENotOwner);
    assert!(!is_active(p, ctx.epoch()) || ctx.sponsor() == option::some(p.guardian), ENotAuthorized);
}

/// First time only: claims the derived UID, so a second call for the same owner aborts.
public fun enable(registry: &mut GuardrailsRegistry, guardian: address, sui_limit: u64, recovery_delay_epochs: u64, ctx: &TxContext) {
    assert!(guardian != ctx.sender(), EGuardianIsOwner);
    assert!(MIN_RECOVERY_DELAY_EPOCHS <= recovery_delay_epochs && recovery_delay_epochs <= MAX_RECOVERY_DELAY_EPOCHS, ERecoveryDelayOutOfRange);
    share(Policy { active_from: ctx.epoch() + 1, active_until: NEVER, limits: {SUI: sui_limit}, .. });
}

/// After `cancel` or once inactive: same as `enable`, on the existing object, keeping its limits.
public fun re_enable(p: &mut Policy, guardian: address, ctx: &TxContext) {
    assert_authorized(p, ctx);
    assert!(p.active_from == NEVER || p.active_until <= ctx.epoch(), EAlreadyActive);
    p.guardian = guardian; p.active_from = ctx.epoch() + 1; p.active_until = NEVER;
}

public fun cancel(p: &mut Policy, ctx: &TxContext) {                 // pending only, key alone
    assert_authorized(p, ctx);
    assert!(p.active_from != NEVER && p.active_from > ctx.epoch(), ENotPending);
    p.active_from = NEVER;
}

public fun disable(p: &mut Policy, ctx: &TxContext) {                // active only, guardian sponsors
    assert_authorized(p, ctx);
    assert!(is_active(p, ctx.epoch()), ENotActive);
    p.active_until = ctx.epoch() + 1;
}

public fun set_limit<T>(p: &mut Policy, limit: u64, ctx: &TxContext)   // at once
public fun remove_limit<T>(p: &mut Policy, ctx: &TxContext)            // at once
```

`enable` takes the SUI limit as a number and the other limits go through `set_limit<T>`, because a
PTB cannot build a `TypeName` from pure arguments.

So a policy is pending from `enable` until `active_from`, during which you can configure or `cancel`
it with the key alone and nothing is enforced. Then it is active until `active_until`, during which
every transaction the account sends is checked, and any change needs the guardian. After `cancel` or
`active_until` it is inactive, the object stays, and `re_enable` starts over. An active policy
cannot be replaced: `enable` cannot claim the UID twice, `re_enable` aborts while active, and only
the guardian can change the guardian, so a key alone cannot take the account out of its policy.

Why `enable` and `disable` wait for the next epoch while limits change at once: execution reads the
policy at the version consensus pinned, so anything only execution reads can change any time.
Whether an account has an active policy is also needed at sequencing and at signing, and there it is
read from the latest state, outside consensus order. That is only deterministic if `is_active` for
the current epoch does not change within the epoch, so activation and deactivation land on epoch
boundaries. Every function above keeps that: none of them changes whether the policy is active in
the epoch it runs in. The pending epoch has a second use: if someone holding your key enables a
policy with their own guardian to lock you out, that is when you see it and cancel it with the key.

## How a transaction finds its policy

A transaction never declares the policy it is subject to, so wallets and apps keep working
unchanged. Consensus adds it:

```rust
/// At sequencing, for every user transaction.
fn implicit_policy_key(tx: &TransactionData) -> Option<(ObjectID, SequenceNumber)> {
    let id = derive_object_id(GUARDRAILS_REGISTRY, PolicyKey(tx.sender()));
    if tx.shared_input_objects().any(|input| input.id == id) { return None }   // declared already
    let policy = object_cache.get_object(id)?;                                  // latest version
    policy.is_active(epoch).then(|| (id, policy.initial_shared_version()))
}
```

The result is added to the transaction's shared inputs as a mutable input. From there the ordinary
shared object path applies: next version table, lamport version, bump after the transaction. The
assigned version travels with the transaction like any shared input, and input loading and the
execution scheduler fetch and wait for the policy like a declared one.

Reading the latest version from the object cache is sound: the only two things consensus takes from
the object are `is_active(epoch)`, which does not change within the epoch, and the initial shared
version, which never changes. There is no separate memo. An account without a policy is a lookup of
an object that does not exist, which the cache answers from its negative entries.

Only active policies become inputs, and a policy activates at least one epoch after it is created.
That is what keeps the checkpoint executor path safe: it reconstructs versions from effects, where a
mutated shared object carries its version but not its initial shared version, and reads the latter
from the store. By the first transaction of the activation epoch every node holds the object.

The cost is that an account's transactions serialize on its policy object, one at a time, and that
each of them rewrites the object. For the accounts we target that is not a limitation.

Everything that reads a transaction's shared inputs sees the policy like a declared input. Before
version assignment one function computes the inputs, the declared ones plus the policy, and
version assignment, congestion control (the deferral decision, the per object cost and the debts
carried between commits), the overload tracker and the gas price suggestion all take them from it.
After version assignment the rule is that every consensus object a transaction was assigned or, in
effects, accessed without declaring it is a mutable input, so the execution scheduler, input
loading, the execution time estimator, the checkpoint executor and replay need no guardrails
knowledge. Without congestion control counting it, anyone could enable a policy on an account of
their own and use it as a shared object that no budget bounds.

Only the sender's policy is taken. The two other ways one account's value can move in a transaction
another account sends are refused at signing instead (see Signing checks), so this stays one policy
per transaction.

## Guardian approval is just sponsorship

The guardian approves a transaction by sponsoring it, i.e. the guardian is the gas owner and signs
it just like any sponsor does today. The gas owner is part of `TransactionData`, so the approval is
covered by the digest, a copy without the guardian signature is simply invalid, and signing,
execution and dry-run all see the approval from the transaction data. Move sees it through
`tx_context::sponsor()`. There is no new signature rule or verification code to write.

This also means any Sui account can be a guardian, since sponsorship goes through the ordinary
signature verifier: passkey, zkLogin, a plain key, or a multisig combining several of them, which is
how you would compose a stronger second factor. The guardian pays gas for what it approves, and its
gas coins are exposed to the PTB like any sponsor's, so a guardian wallet should review what it
signs. A guardian can have an active policy of its own. It then approves only transactions that do
not use the gas coin as an argument (see Signing checks), which approvals of the owner's
transactions do not need.

An exempt transaction is not counted: whatever leaves the account with the guardian's approval does
not touch the budget. The policy object is still an input of the transaction and bumps its version,
but its contents do not change. Whether a transaction is exempt is judged on the policy as the
transaction found it, so a guardian sponsored transaction that changes the guardian stays exempt.

The one limitation: a transaction cannot have both a third party gas sponsor and a guardian.
Approved transactions should be rare, so we are fine with it.

## Guardian rotation and recovery

Rotation is easy: `set_guardian` goes through `assert_authorized` like every other change, so while
the policy is active the current guardian sponsors it, and it takes effect at once.

Losing the guardian is the real problem. If the guardian key is gone, every change to an active
policy aborts, and the account is pinned to its limits forever. So the key alone needs a way out
that a stolen key cannot abuse. The answer is a time lock: `request_disable` can be called with the
key alone, even while active, and sets `active_until = e + recovery_delay_epochs`. Only the guardian
can cancel it. A stolen key that requests it only tells the owner, whose guardian cancels it. A lost
guardian costs the owner the delay, during which the policy still holds.

```move
public fun set_guardian(p: &mut Policy, guardian: address, ctx: &TxContext) {
    assert_authorized(p, ctx);                                      // guardian sponsors while active
    assert!(guardian != p.owner, EGuardianIsOwner);
    p.guardian = guardian;
}

public fun request_disable(p: &mut Policy, ctx: &TxContext) {       // key alone, no guardian
    assert!(p.owner == ctx.sender(), ENotOwner);
    assert!(is_active(p, ctx.epoch()), ENotActive);
    assert!(p.active_until == NEVER, EDisablePending);
    p.active_until = ctx.epoch() + p.recovery_delay_epochs;
}

public fun cancel_disable(p: &mut Policy, ctx: &TxContext) {        // guardian only
    assert!(p.owner == ctx.sender(), ENotOwner);
    assert!(ctx.sponsor() == option::some(p.guardian), ENotGuardian);
    assert!(p.active_until != NEVER && p.active_until > ctx.epoch(), ENoPendingDisable);
    p.active_until = NEVER;
}
```

`request_disable` aborts when an end is already set. Otherwise a stolen key could push the owner's
own pending recovery further out by requesting it again. The delay has an upper bound so the
addition cannot overflow.

## The check at execution

The check runs in the adapter after the PTB finished and before storage is metered, with the policy
already loaded as an input at the version consensus pinned.

```rust
fn check_guardrails(tx) -> Result<(), ExecutionError> {
    let Some(policy) = input_policy(tx.sender) else { return Ok(()) };   // consensus added it only if active
    if !policy.is_active(epoch) || tx.gas_owner == policy.guardian { return Ok(()) }
    ensure!(policy_is_mutable_input, GuardrailsViolation { kind: PolicyNotWritable });
    ensure!(no_party_coin_input, GuardrailsViolation { kind: PartyCoin });

    for (ty, amount) in sender_outflow(tx.sender) {                       // the formula in The rule
        let spent = if policy.spent_epoch == epoch { policy.spent[ty] } else { 0 };
        ensure!(spent + amount <= policy.limits.get(ty).unwrap_or(0),
                GuardrailsViolation { account: tx.sender, kind: BudgetExhausted { coin_type: ty } });
        record(ty, amount);                                               // resets `spent` first if the epoch moved on
    }
    write_back(policy);
}

ExecutionFailureStatus::GuardrailsViolation { account: SuiAddress, kind: GuardrailsViolationKind }

enum GuardrailsViolationKind {
    /// The epoch budget of `coin_type` has no room for this transaction's outflow.
    BudgetExhausted { coin_type: TypeTag },
    /// The transaction declares the policy object without write access.
    PolicyNotWritable,
    /// The transaction takes a coin with party owners, which the rule does not count.
    PartyCoin,
}
```

The check is exact: the pinned policy holds every earlier transaction's spend, since they all wrote
it. So there is no per transaction cap, no reservation and nothing to decide before execution.

A violation fails the transaction like any other execution error: its writes are dropped, including
the policy write, and gas is charged. Nothing is recorded for a failed transaction, because nothing
left the account but gas, and gas is not counted. The write happens before storage is metered, so it
is paid like any other write. A transaction that takes nothing out does not write the policy; the
object only bumps its version like every mutable input.

The check runs before gas is charged, when the gas coin still holds its full value, so gas does not
show up as outflow.

## Signing checks

Validators refuse to sign these. Simulation runs them too, and dev-inspect does not.

```rust
// Nobody but its owner declares a policy, and only as a mutable input. Every transaction of the
// owner depends on the object, so nobody else gets to contend on it.
for input in tx.shared_input_objects() where input is a Policy {
    ensure!(policy.owner == tx.sender() && input.is_mutable());
}
// A gasless transaction may not write an object, and the check records the spend in the policy.
ensure!(!(tx.is_gasless() && has_active_policy(tx.sender())));
// Execution fails it too, since a coin can become party owned after signing. Refusing it here
// saves the sender the gas.
ensure!(!(has_active_policy(tx.sender()) && tx.shared_inputs().any(is_party_owned_coin)));
// Apart from a withdrawal, a transaction reaches its gas owner's SUI beyond the gas it pays only
// through the gas coin argument. A gas owner with a policy loses at most the gas budget of a
// transaction that uses neither.
ensure!(tx.gas_owner() == tx.sender() || !tx.uses_gas_coin() || !has_active_policy(tx.gas_owner()));
// No withdrawal from another account with a policy, as the sponsor or under an allowance.
for account in tx.withdrawals().filter(|w| !w.is_seizure()).map(|w| w.account()).filter(|a| a != tx.sender()) {
    ensure!(!has_active_policy(account));
}
```

The last two are what makes "only the sender's policy" enough. A stolen key can sign as the gas
owner of the attacker's transaction, and that transaction can take SUI straight out of the gas coin.
It can also grant an allowance that the attacker then withdraws from, or name itself as the sponsor
of a withdrawal. All of these move the account's value in a transaction the account did not send.
So an account with an active policy can pay the gas of another account's transaction only if the
transaction leaves the gas coin alone, and no transaction of another account can withdraw from it.

A third way exists and is out of scope: the issuer of a regulated coin can seize it from a frozen
address (`WithdrawFrom::FrozenAddress`). The policy bounds the account's key, and seizure does not
use it.

The checks on gasless transactions, the gas owner and withdrawals depend only on whether a policy is
active this epoch, which does not change within the epoch, so every validator decides the same. The
check on party coins depends on the latest owner of an input, and execution repeats it. The check on
declared policies depends on whether a declared object is a policy, which changes when a policy is
created. A policy created in the current epoch is not active yet, so a validator that
signs before it sees the policy only lets another account contend on a policy that no transaction
of its owner depends on yet.

## Effects, replay, dry-run

Effects show the policy as a mutated shared object the transaction did not declare. That is a new
rule for everything that derives inputs from a transaction:

- The checkpoint executor and crash recovery take the policy's version from effects and its initial
  shared version from the store, which the activation delay guarantees is there.
- Replay takes the policy as an input when effects show it.
- Dry-run, dev-inspect and simulacrum add it from the latest state, so the check runs there too
  and a wallet can show the user what would happen. A guardian sponsored dry-run is exempt just
  like the real thing.
- Indexers and SDKs that compute "objects touched" from the transaction body need to learn the
  rule; ones that read effects are unaffected.

The new execution status is an on-wire change and goes through the two-PR rule; the feature flag
stays devnet only until then.

## What this version leaves out

Each of these can be added later without changing the policy object or how a transaction finds it.

- **Value outside coins and the address balance.** #28682 tracks the `Balance<T>` totals of every
  object a transaction reads and writes. On top of it the rule can count every balance rooted at
  the account, which protects stakes and wrapped balances and stops charging the owner for moving
  value into objects they keep. It needs root resolution over owners and wrapped UIDs, and it has
  to deal with dynamic fields the transaction never loads.
- **Allowed recipients.** A list of addresses that receive without limit, for your own other
  accounts. Until then the guardian sponsors those transfers.
- **Gas in the budget.** It needs the spend written on the failure path too, after the transaction's
  writes are dropped, next to the gas coin handling.
- **The account as the gas owner of a transaction that uses the gas coin, or as the source of
  another account's withdrawal.** Refused today. Supporting it means taking more than one policy
  per transaction.
- **Coins with party owners.** Refused today. Counting them needs a rule for which owner a coin
  shared by several accounts counts against.
- **Gasless transactions.** Refused today. Supporting them means letting a gasless transaction
  write the policy and deciding who pays for that write.

## The alternative we did not take: reserve before execution

Limits have to bound an epoch, not a transaction, so the spend of the epoch has to live somewhere
every node agrees on. We keep it in the policy object and make the policy a mutable input.

The other complete design does what address balance withdraws do: decide before execution, in
consensus order, from a declared upper bound. Counters live in accumulator fields that native
settlement resets per epoch, the funds withdraw scheduler reserves a per transaction cap against
them, and a transaction that does not fit is cancelled without executing. The owner's transactions
keep running in parallel, and the transaction model does not change.

We chose the implicit input by where a bug would come from and what it would cost:

- The reservation design spreads the logic over the funds withdraw scheduler, native settlement and
  execution. Its correctness rests on timing invariants across the three. A bug in any of them makes
  validators reach different cancel decisions, which is a fork, and a silent one until effects
  disagree.
- The implicit input design keeps the logic in the adapter and the version manager. The version
  manager's one invariant, that `is_active` is constant within an epoch, is easy to state and
  check. The execution side is synchronous and reads a pinned object, so a bug there is a wrong
  check, which is deterministic and shows up in a single process test.

We take the one whose failure mode is loud and local, and we accept the serialization of the owner's
transactions and one new rule in the transaction model for it.

## Things we considered and rejected

1. Declaring the policy as an ordinary input of every transaction, enforced at signing. No changes
   to consensus at all, but every wallet and app that builds a transaction for the account would
   have to add the input, which breaks the ecosystem for these accounts.
2. One registry with every policy as a dynamic field. The owner's transactions have to write the
   spend, and a dynamic field of one registry would serialize every guardrailed account in the
   network on that registry, so each account needs its own object.
3. A per epoch memo of which accounts have an active policy. The object cache already answers the
   question, including for accounts without a policy, and the memo grew by one entry per address
   seen.
4. Guardian approval as an extra signature in the envelope. The digest only covers
   `TransactionData`, so anyone seeing the envelope can resubmit without the guardian signature.
   Fixing it means declaring the co-signer in `TransactionData`, i.e. a new transaction format, and
   sponsorship already declares the second signer.
5. Guardian approval as a signature passed as PTB data. The signature can't cover a digest that
   contains itself, and on-chain verification is limited to Move natives: no zkLogin, no multisig.
6. Guardian approval as a separate transaction that records an approved digest in the policy. Two
   transactions, digests go stale when the gas coin moves, and a list that needs expiry and pruning.
7. A per transaction limit instead of per epoch. Not a bound at all.
8. A lifetime counter capped at `limit × epochs since activation`. Unspent budget rolls over, so an
   account that rarely transacts, exactly our target user, accumulates budget that a stolen key
   drains at once.
9. A package allowlist with custody rules for non-coin objects. That's a later layer.

## Implementation notes

- Implementation note: `sui_core::consensus_inputs` owns which shared objects a transaction takes.
  The consensus handler computes each transaction's shared inputs once per commit for congestion
  control, version assignment computes them again, and the overload check at submission and the gas
  price suggestion compute their own. Each computation is one object cache read, a hit or a
  negative hit.
- Implementation note: after version assignment, and in effects, a consensus object a transaction
  does not declare is a mutable input, outside the implicitly read system objects. Only a
  guardrails policy can be one today.
- Implementation note: the execution scheduler's barrier dependencies for non-exclusive writes read
  only declared inputs. No dependency is missed, because only the owner declares a policy and only
  as a mutable input, so nothing takes a non-exclusive write on it.
- Implementation note: backtest does not add the policy input yet.
- Implementation note: the registry is created at genesis only. A network that started without it
  needs an end-of-epoch creation transaction, which is a follow-up.
- Implementation note: the signing checks for gasless transactions and allowance funders have no
  test yet.
