# Address Balances: Design Documentation

This directory contains the design documentation for Sui's **address balances** system —
the accumulator-based mechanism that lets addresses (and objects) hold virtual `Balance<T>`
amounts directly on a single shared root object, without spawning per-coin objects.

If you are touching code under `crates/sui-core/src/accumulators/` or
`crates/sui-core/src/execution_scheduler/funds_withdraw_scheduler/`, this is the place to start.

## What's in this directory

| Doc | Topic |
|-----|-------|
| [`data_model.md`](./data_model.md) | On-chain layout: the `0xACC` root, `AccumulatorObjId` derivation, dynamic-field representation, and the version invariant. |
| [`write_path.md`](./write_path.md) | How a balance changes: `FundsWithdrawalArg`, transaction rewriting, accumulator events (Split/Merge), `AccumulatorSettlementTxBuilder`, and the placeholder→settlement-txns→barrier expansion. Also covers the two paths by which settlement reaches a node (validator vs. checkpoint executor). |
| [`address_funds_scheduling.md`](./address_funds_scheduling.md) | Pre-execution withdraw scheduling for **address-owned** accumulator accounts (max amounts known up front). Eager scheduler, naive baseline, determinism. |
| [`object_funds_in_execution.md`](./object_funds_in_execution.md) | In-execution sufficiency checking for **object-owned** accumulator accounts, inside the Move VM (`check_object_funds_withdraw_in_execution`). The current design. |
| [`object_funds_checking.md`](./object_funds_checking.md) | **Deprecated** post-execution sufficiency checking for object-owned accounts, used only while the in-execution flag is off. |
| [`coin_reservations.md`](./coin_reservations.md) | Backward-compat layer that lets pre-address-balance SDKs use address balances by encoding withdrawals as fake `ObjectRef`s. Transitional — expected to be removed once SDK migration is done. |

The **read path** (RPC balance queries via `accumulators/balances.rs` and the
`AccountFundsRead` trait in `accumulators/funds_read.rs`) is small enough that the inline rustdoc
on those modules is the source of truth — no separate doc is provided here.

## High-level architecture

```
                  ┌──────────────────────────────────┐
                  │         User Transaction         │
                  └──────────────┬───────────────────┘
                                 │
                                 ▼
                Has declared address withdraws?
                                 │
                          ┌──────┴──────┐
                         YES            NO
                          │             │
                          ▼             │
              ┌────────────────────────┐│
              │ FundsWithdrawScheduler ││
              │ (PRE-execution gate)   ││
              │ — see address_funds_   ││
              │   scheduling.md        ││
              └────────────┬───────────┘│
                           │            │
                           ▼            ▼
                  ┌──────────────────────────────────┐
                  │     execution engine/adapter     │
                  │  emits AccumulatorEvents per     │
                  │  Split/Merge — see write_path.md │
                  └──────────────┬───────────────────┘
                                 │
                                 ▼
                      Has object withdraws?
                                 │
                          ┌──────┴──────┐
                         YES            NO
                          │             │
                          ▼             │
              ┌────────────────────────┐│
              │  Sufficiency check     ││
              │  in-execution (VM) —   ││
              │  see object_funds_in_  ││
              │  execution.md; legacy  ││
              │  post-execution — see  ││
              │  object_funds_checking ││
              └────────────┬───────────┘│
                           │            │
                           ▼            ▼
                  Effects committed; events
                  accumulated for settlement
                                 │
                                 ▼
                  ┌──────────────────────────────────┐
                  │  Settlement: N parallel          │
                  │  settlement txns + barrier tx    │
                  │  (bumps accumulator version)     │
                  │  — see write_path.md §4          │
                  └──────────────────────────────────┘
```

## Glossary

- **Accumulator root (`0xACC`)** — the singleton shared object
  (`SUI_ACCUMULATOR_ROOT_OBJECT_ID`) that owns every virtual balance as a dynamic field.
- **Accumulator version** — the `SequenceNumber` of the accumulator root. Bumped exactly once
  per settlement batch, by the *barrier* transaction.
- **`AccumulatorObjId`** — the `ObjectID` of an individual accumulator account (a dynamic field
  under `0xACC`). Derived deterministically from `(owner_address, balance_type)`.
- **Address funds** — virtual balances owned by an address (`SuiAddress`). Withdraw maximums are
  declared in transaction data; sufficiency can be checked **before** execution.
- **Object funds** — virtual balances owned by another object (UID). Withdraw amounts are
  determined by Move program logic at runtime; sufficiency must be checked **after** execution.
- **Settlement** — the act of folding a commit's accumulator events into the on-chain root.
  Implemented as N parallel settlement transactions (`NonExclusiveWrite` access) plus one
  barrier transaction (`Mutable` access).
- **Barrier transaction** — the final settlement transaction that gates the accumulator version
  bump. Driven by `accumulators::build_accumulator_barrier_tx`.
- **Split / Merge** — Accumulator events. Split = withdrawal, Merge = deposit.
- **Running max withdraw** — the peak net withdrawal an account experiences during a single
  transaction's execution. The basis for object-funds sufficiency checking.
- **Reservation** — in the address-funds scheduler, the *declared maximum* a transaction may
  withdraw from an account. Held against the in-memory balance until settlement releases it.
- **Checkpoint-builder path / checkpoint-executor path** — the two ways settlement reaches a validator.
  Documented in [`write_path.md`](./write_path.md) §5 and consumed everywhere else.

## Conventions used in these docs

- Code references prefer symbol names such as `module::Type` or `Type::method`; line-specific
  references are used sparingly when the exact site matters. Where a function is named, it's the
  source of truth — the doc should follow the code, not the other way around.
- Diagrams are ASCII, kept simple enough to edit in place.
- "Sections N.M" cross-references are within a single doc; cross-doc references give the file
  path explicitly.
