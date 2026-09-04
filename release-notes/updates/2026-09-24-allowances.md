---
title: Allowances, a native payments primitive
date: 2026-09-24
summary: One address can now authorize another to spend from its balance, within limits the owner sets and can revoke at any time.
---

Sui now has allowances: an address can authorize another address to spend from
its balance, up to a limit the owner sets and can revoke at any time. The owner
keeps custody throughout. No funds move to the spender, and nothing is escrowed.

## Why this is a primitive and not a standard

Allowances ship as part of the Sui Stack rather than as a smart contract
standard. That distinction is the point.

A standard is a convention that each app implements for itself. Every wallet
that wants to show a user their outstanding allowances has to know which
contracts implement the convention and how each one stores its state. Bounded
spend gets rebuilt per app, with a different shape each time, and tooling can
only support the implementations it has been taught about.

As a first-class primitive, allowances are a single object type. Any wallet,
app, or agent issues and reads them the same way, whatever created them. A
wallet can list every allowance an address has granted without integrating with
each protocol that requested one.

## What it unlocks

- **Recurring payments.** A subscription can draw a fixed amount on a schedule
  without holding the payer's funds between charges.
- **Budgets.** A treasury can cap what a team or a service is able to spend
  without moving assets into a separate account first.
- **Agent-initiated spend.** An agent can transact on a user's behalf within a
  ceiling the user sets, which is the difference between delegating a budget and
  handing over a key.

The common thread is bounded authority. Each of these was previously built by
wrapping funds in a custom contract, which means the user's assets leave their
control and the security of the arrangement depends on that contract. An
allowance is a grant against a balance the owner still holds.

## Revocation

An owner can revoke an allowance at any time. Because the funds never left the
owner's balance, revocation is immediate and does not depend on the spender
cooperating or on a withdrawal completing.
