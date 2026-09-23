---
title: Allowances, a native payments primitive
date: 2026-09-24
summary: Allowances let one address authorize another to spend from its balance, within limits the funder sets and can revoke at any time.
---

Sui supports [allowances](/onchain-finance/allowances), a native payments primitive that lets one address authorize another to spend from its [address balance](/onchain-finance/asset-custody/address-balances), within limits the funder sets and can revoke at any time.

Allowances ship in the Sui framework rather than as a smart contract standard, so any wallet, app, or agent issues and reads them the same way. Funds stay in the funder's balance until a spend succeeds, which makes an allowance a permission rather than a deposit. Nothing is escrowed, nothing is pre-funded, and an unused allowance costs nothing.

## Why this matters

Every payment from an address normally needs that address's signature. That rules out anything where someone else pulls funds on a schedule or within a budget while the owner is offline. Allowances lift that constraint without moving custody, and they give you one object type to build on instead of reinventing bounded-spend logic in each app.

That covers subscriptions, virtual budgets and family allowances, agentic and burner wallets that spend against a main wallet, merchant pre-authorizations, and revocable payment authorizations that behave like checks. [What Are Allowances?](/onchain-finance/allowances/allowances-overview) has the full list.

## How it works

The funder calls `sui::allowance::new` from a [programmable transaction block](/develop/transactions/ptbs/prog-txn-blocks) (PTB), naming the spender, the coin, and the limits. Sui creates the allowance as a [shared object](/develop/objects/object-ownership/shared) and sends the funder an `AllowanceCap`. That capability carries only the `key` ability, so nobody can transfer or wrap it away from the funder.

The spender then sends an ordinary transaction that declares a withdrawal from the funder under that allowance. The spender signs it and pays for it, and the funder does not need to be online. Sui completes the spend only when the allowance is valid and the amount fits its limits.

You bound an allowance by lifetime cap, rate limit, start time, expiration, and coin type. Every allowance carries at least one bound on amount and one on time.

A direct allowance names a spender whose signature is enough to pull funds. An app-bound allowance routes each spend through a specific app's Move module, which can change the spender without involving the funder. In both cases the funder revokes at any time with `sui::allowance::revoke`, and revocation is immediate and final.

## Get started

Allowances need Sui v1.80 or later on Devnet and Testnet, v1.81 or later on Mainnet, and `@mysten/sui` 2.31 or later for the TypeScript SDK.

- [What Are Allowances?](/onchain-finance/allowances/allowances-overview) covers the concepts, the use cases, and the limits you can set.
- [Using Allowances](/onchain-finance/allowances/using-allowances) covers the API, Move and TypeScript examples, and the app-bound flow.
- [Allowances FAQ](/onchain-finance/allowances/allowances-faq) covers rate limit windows, timestamps, and what happens when a spend fails.
- [`sui::allowance`](/references/framework/sui_sui/allowance) is the module reference.
