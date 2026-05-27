# Coin Reservations: Backward Compatibility for Pre-Address-Balance Clients

This document covers the **coin reservation** compatibility layer — the mechanism that lets
SDKs and wallets that predate address balances continue to send transactions even when the
sender's wallet contains *only* address balance and no coin objects to use as gas or inputs.

This is a **transitional** subsystem. The goal is to be invisible to old clients: they think
they're seeing a regular `Coin<T>` object and using it normally, while the validator-side and
RPC-side code translates back and forth between the fake coin object and the real address
balance. Once enough of the ecosystem migrates to native address-balance support, this layer is
expected to be removed.

For the on-chain data layout, see [`data_model.md`](./data_model.md). For where coin
reservations enter the write path proper, see [`write_path.md`](./write_path.md) §1.

## 1. The problem

A user's wallet may contain only address balances (virtual `Balance<T>` amounts under the
accumulator root) and no `Coin<T>` objects. An old SDK that doesn't understand address
balances has only one way to express "spend some funds": construct a `CallArg::Object` /
`Gas::ImmOrOwnedObject` reference to a coin. With no real coin objects available, the SDK
would have nothing to point at, and the wallet would appear empty.

The compatibility layer solves this by **synthesizing a fake `ObjectRef` that *encodes* an
address-balance withdrawal**. From the SDK's perspective it's just a coin. From the
validator's and RPC's perspective, the magic value in the object's digest identifies it as a
disguised withdrawal and triggers a translation pipeline.

## 2. The encoding

A coin-reservation `ObjectRef` is laid out exactly like a regular owned-object reference:

```
(ObjectID, SequenceNumber, ObjectDigest)
```

with all three components carrying meaning:

### 2.1 `ObjectID` — masked accumulator object ID

The `ObjectID` is the `AccumulatorObjId` of the underlying balance (which is itself derived
from `(owner, type)` — see [`data_model.md`](./data_model.md) §3), **XOR-masked with the
network's chain identifier** (the genesis checkpoint digest):

```rust
pub fn mask_or_unmask_id(object_id: ObjectID, chain_identifier: ChainIdentifier) -> ObjectID {
    // 32-byte XOR
}
```

(`sui-types/src/coin_reservation.rs:183`.) XOR is its own inverse, so the same function masks
and unmasks. The mask serves two purposes:

1. **Cross-chain replay protection.** Without masking, a signed transaction containing a fake
   coin ref against accumulator object `V` on chain A could be replayed against the same `V`
   on chain B — and an attacker could pre-mine an address whose accumulator id matches a
   target `V` on the foreign chain. With masking, the on-chain object id the validator
   actually checks is `V ⊕ chain_id`. To replay against another chain, an attacker would have
   to mine `(addr, type)` such that `dynamic_field_key(addr, type) ⊕ FOREIGN_CHAIN_ID =
   TARGET_ACCUMULATOR_ID ⊕ TARGET_CHAIN_ID` — a 256-bit collision search per chain, which is
   not feasible.
2. **A natural "is this a fake?" detector for read APIs.** An RPC server that's asked to read
   the masked id will find no such object (the real object lives at the unmasked id). The
   server can then unmask, retry, and — if the second read hits an accumulator account —
   conclude that the original id was a coin-reservation reference. See §4 for how that's used.

### 2.2 `SequenceNumber` — version hint

The `SequenceNumber` field is the version of the accumulator root object at the time the
fake ref was synthesized. The protocol does **not** read it; it exists purely to help old
clients that key their object-cache entries on `(id, version)`. Treat it as opaque.

### 2.3 `ObjectDigest` — packed payload

The `ObjectDigest` (32 bytes) is repurposed to carry the actual reservation payload, in three
fixed-offset fields:

| Bytes  | Field                | Notes                                                             |
|--------|----------------------|-------------------------------------------------------------------|
| 0..8   | `reservation_amount` | `u64` little-endian. The maximum the tx may withdraw.             |
| 8..12  | `epoch_id`           | `u32` little-endian. The epoch in which the tx is valid.          |
| 12..32 | magic constant       | `0xac` × 20. Used to recognize a coin-reservation digest.         |

The magic constant gives `ParsedDigest::is_coin_reservation_digest` a cheap check
(`sui-types/src/coin_reservation.rs:95`). The 4-byte epoch field is a *very* generous
budget — at one epoch per day it's more than 12 million years.

The encode/decode roundtrip lives in `sui-types/src/coin_reservation.rs:107-137`
(`ParsedDigest <-> ObjectDigest`) and `:170-181`
(`ParsedObjectRefWithdrawal::parse` / `::encode`).

## 3. The pipeline

A coin-reservation `ObjectRef` flows through the system in a fixed sequence:

```
   Old client / SDK
        │
        │ get_all_coins(owner)  ──────►  RPC returns synthesized
        │ get_object(masked_id)          fake Coin<T> objects
        │                                (see §4 below)
        ▼
   Construct tx using
   the fake ObjectRef as
   gas or as an input
        │
        ▼
   ┌──────────────────────────────────────────┐
   │ Signing-time validation                  │
   │  - validity_check (transaction.rs:830)   │
   │  - is_coin_reservation_digest gate       │
   │  - sender == owner check on accumulator  │
   │  - gas-only: must be Balance<SUI>,       │
   │    sponsorship not allowed               │
   └────────────────┬─────────────────────────┘
                    │
                    ▼
   ┌──────────────────────────────────────────┐
   │ Execution-time rewriting                 │
   │  rewrite_transaction_for_coin_           │
   │    reservations (transaction_rewriting   │
   │    .rs:19)                               │
   │  fake ObjectRef ──► CallArg::FundsWith-  │
   │    drawal(FundsWithdrawalArg{...})       │
   └────────────────┬─────────────────────────┘
                    │
                    ▼
   Same write path as native address balances
   (see write_path.md §1)
```

### 3.1 Read-side: what RPC returns

The synthesized refs are produced by RPC code, not by users. The two main paths are:

- **`AuthorityState::get_address_balance_coin_info(owner, balance_type)`**
  — looks up the on-chain accumulator object for `(owner, type)`, reads
  the current balance, and calls `coin_reservation::encode_object_ref` to package it into a
  fake `ObjectRef`. Returns `(fake_ref, balance, last_tx_digest)`.
- **`AuthorityState::get_all_address_balance_coin_infos(owner)`** — same idea, but iterates every
  coin type the owner
  has a balance in.

These are wired into `getCoins`, `getAllCoins`, `getObject`, etc. from the
`sui-json-rpc::authority_state` shim layer.

### 3.2 Read-side: handling `getObject(masked_id)`

When a client calls `getObject` with a masked id, the on-chain object doesn't exist (the real
object lives at the unmasked id). The fallback in
`sui_json_rpc::authority_state::get_object_read` implements the two-phase trick from §2.1:

1. Try the requested id. If it exists, return it (i.e., the masked id collided with a real
   object — vanishingly unlikely, but handled).
2. If not, unmask using the local chain id and try again.
3. If the second read hits a balance accumulator field, synthesize a fake `Coin<T>` object
   (an `Object` whose `MoveObject` is a coin with the masked id and the current balance)
   and return *that*. Otherwise return the original `NotExists`.

Step 3 is what makes the layer transparent to old clients: they ask for the coin they were
told about, and get back something that walks like a coin and quacks like a coin, even though
no such on-chain object actually exists.

### 3.3 Signing-time validation

`CallArg::validity_check` gates coin-reservation refs behind the
`enable_coin_reservation_obj_refs` protocol flag. If the flag is off and a digest matches the
magic, the transaction is rejected with `Unsupported`.

For gas, the validity check is stricter inside `TransactionData::check_gas`:

- The reservation must be for `Balance<SUI>` specifically (the unmasked id must equal
  `AccumulatorValue::get_field_id(sender, Balance<SUI>)`).
- `gas_owner == sender` — sponsorship is **not** supported via coin reservations. (Native
  `FundsWithdrawalArg::balance_from_sponsor` *does* support sponsorship; the restriction is
  specific to the legacy path.)

Validity checks that need owner/type information also resolve the unmasked accumulator object
through `CoinReservationResolver` (`sui-types/src/coin_reservation.rs:210`). The
`CachingCoinReservationResolver` (`accumulators/coin_reservations.rs:20`) wraps it with a
moka cache: `(owner, type_tag)` for an accumulator never changes once the object exists, so
successful lookups and certain permanent errors are cached. **Transient `not_found` is not
cached** — the accumulator may not exist on this node yet but will appear once the relevant
settlement transaction executes; caching the miss would poison the cache.

### 3.4 Execution-time rewriting

`accumulators::transaction_rewriting::rewrite_transaction_for_coin_reservations` is invoked just
before execution. It walks the PTB inputs; for each
`CallArg::Object(ImmOrOwnedObject(ref))` whose digest parses as a coin-reservation digest, it:

1. Parses the ref into `ParsedObjectRefWithdrawal`.
2. Calls `coin_reservation_resolver.resolve_funds_withdrawal(sender, parsed,
   accumulator_version)` to get the canonical `FundsWithdrawalArg`. The
   `accumulator_version` matters during checkpoint replay: we read the accumulator at the
   version *before* any settlement transaction in the same checkpoint has modified it.
3. Replaces the input with `CallArg::FundsWithdrawal(...)`.

After this rewrite the transaction looks identical to one a modern SDK would have built,
and from `process_funds_withdrawals_for_execution` onwards (see
[`write_path.md`](./write_path.md) §1) there is no longer any distinction.

Note that the gas path does **not** go through this rewriter today — gas coin reservations
are detected and accumulated inside `process_funds_withdrawals_for_execution` directly
(`TransactionData::process_funds_withdrawals_for_execution`). The two paths converge at the
per-account reservation map.

## 4. Why this design

A few choices in the encoding are worth calling out:

- **Why XOR instead of an HMAC or signature?** XOR is symmetric (one function masks and
  unmasks), free in CPU, and sufficient: the security argument is collision resistance on
  256-bit ids, not unforgeability. An attacker can construct any masked id they like; what
  they cannot do is make it resolve to *someone else's* real accumulator on a target chain.
- **Why pack the amount into the digest?** A coin reservation needs to carry an amount so the
  validator knows the cap; the digest is the only field with 32 free bytes. The remaining
  20 bytes go to the magic constant, which both signals "this is a reservation" and
  squeezes the chance of a collision with a real object digest into negligible territory.
- **Why a magic byte pattern instead of a typed enum?** Because the wire format must be a
  raw `ObjectDigest` — old clients and serialization paths don't have a "or a reservation
  envelope" branch to look at. The magic is the cheapest in-band tag that fits.
- **Why no sponsorship for gas reservations?** Sponsorship would mean someone *other than*
  the sender pays gas. The legacy path's encoding only carries one address (the unmasked
  accumulator id, which always belongs to the sender), so sponsorship can't be expressed
  here without a wire-format change. Modern transactions that need sponsored gas should use
  the native `FundsWithdrawalArg::balance_from_sponsor` path instead.

## 5. Lifecycle

This whole layer is intended to be temporary. Once enough SDKs and wallets natively support
`FundsWithdrawalArg`, the protocol-config flag (`enable_coin_reservation_obj_refs`) can be
disabled and the supporting code removed. The natural cleanup order:

1. RPC stops returning fake coin refs (`get_address_balance_coin_info` and friends).
2. The protocol flag is turned off — signing-time validation rejects any remaining
   coin-reservation digests.
3. After a deprecation window, the rewriting code, the `coin_reservation` module, and this
   doc are deleted.

Until then: treat all of `coin_reservation.rs`, `accumulators/coin_reservations.rs`, and
`accumulators/transaction_rewriting.rs` as belonging to one cohesive subsystem documented
here.
