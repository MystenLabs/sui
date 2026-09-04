---
title: Allowances, a native payments primitive
date: 2026-09-24
---

Sui is launching Allowances, a native payments primitive that lets one address authorize another to spend from its balance, within limits the owner sets and can revoke at any time.

Allowances ship as a first-class part of the Sui Stack rather than a smart contract standard, so any wallet, app, or agent can issue and read them the same way.

This is a foundational primitive. It unlocks recurring payments, budgets, and agent-initiated spend without custody risk, and it gives you a single object type to build on instead of reinventing bounded-spend logic per app.
