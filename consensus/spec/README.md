<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 formal specification

This directory contains a Lean 4 proof model for Mysticeti v3.

The specification has three parts:

- `lean/` contains the machine-checked definitions and theorems.
- `docs/` contains the assumption ledger, proof scope, and implementation gap
  report.
- `design/` contains designs for changes that close open proof obligations.

The safety proof covers leader decisions, transaction decisions, threshold
arithmetic, the Core garbage-collection boundary, v3 sub-DAG retention, and the
signed transaction vote cutoff. The proof keeps live DAG evidence separate from
the finalizer's buffered committed-prefix evidence.

The liveness proof suite covers consensus progress, liveness for old leader blocks,
commit-index progress, and durable transaction decisions. These results use one
shared assumption catalog. Each theorem uses only the assumptions that apply to its
result.

Commit progress recovery is one liveness component. Its executable status-level
model proves that an in-range window of `depth + 1` usable anchor rounds makes the
complete descending `FlexCommitter` scan find a commit candidate and increase its
modeled commit index. The process model now also proves how local recovery entry,
next-round block flow, quorum block layers, timely first-slot voting, and an
eventual favorable leader-order window compose to commit-index progress. The
recovery policy is not yet implemented in Rust, and the Rust-to-Lean state mapping
is not machine checked.

The recovery proof defines its common layer frontier as the maximum last signed
round among the recovery quorum. It proves the finite exact-next path to that
frontier when the causal parent interval is available. The proposed recovery
parent rule disables score-based ancestor exclusion for the immediate parent round.
It includes the unique available block from each validator, but it still omits a
validator when the local DAG already knows an equivocation. This rule is not
implemented.

The proof model separates the validator set, the leader schedule, the round leader
selection, and each selected leader slot. Current v3 uses the full leader schedule
as the round leader selection in every pending leader round. The general schedule
and selection lemmas do not assume this equality.

## Shared assumptions

The [assumption ledger](docs/ASSUMPTIONS.md#shared-proof-model) is global to the
proof suite. It includes the fault bounds, common epoch configuration,
authentication, post-GST delivery, local processing, task fairness, data
availability, leader schedule viability, round leader selection coverage, and the
leader-order sampling model.

Most of these conditions are standard BFT or partial-synchrony conditions. Two
conditions need special attention:

- Local processing has a positive finite bound `epsilon`, with `epsilon < delta`.
  This processor-speed condition is stronger than message partial synchrony alone.
- The liveness model treats each round's complete leader-slot order as an
  independent uniform permutation. All validators still compute the same order.
  This is an accepted model of the deterministic seeded shuffle.

Useful-peer data retention is not a base assumption for steady-state consensus. It
is needed only when a lagging or restarted validator must fetch old consensus
blocks or commits. In that case, a correct peer must supply the data, or verified
commit sync must move the validator past the point that needs it. Transaction
payloads do not need this retention rule because a validator or user can resubmit
them.

The schedule bounds `f + c < S` and `A <= P_r` are protocol configuration
conditions. They are not network assumptions. Current v3 has `P_r = S` for each
pending leader round. The optional bound `P_r <= Q` limits work; it is not a safety
or liveness requirement.

The model uses Lean only. It does not use mathlib. The project pins Lean 4.33.0 in
`lean/lean-toolchain`.

## Install Lean

This proof needs Lean 4.33.0 and Lake. Elan installs both tools and selects the
version from `lean/lean-toolchain`.

First, check whether Elan and Lean are already available:

```sh
elan --version
lean --version
lake --version
```

If all three commands succeed, you do not need to install Lean again. Run the
commands from `consensus/spec/lean` to make Elan select the pinned Lean version.

### macOS and Linux

Use the Elan installer from the [Lean install page](https://lean-lang.org/install/):

```sh
curl https://elan.lean-lang.org/elan-init.sh -sSf | sh
```

Start a new shell after the installer completes. You can also load the Elan path in
the current shell:

```sh
source "$HOME/.elan/env"
```

Then install the exact version that this proof uses:

```sh
elan toolchain install leanprover/lean4:v4.33.0
```

### Windows

Download and run `elan-init.exe` from the
[Elan release page](https://github.com/leanprover/elan/releases). Start a new
PowerShell window after the installer completes. Then install the pinned version:

```powershell
elan toolchain install leanprover/lean4:v4.33.0
```

### Verify the installation

The first `lake build` command reads `lean-toolchain`. Elan then installs
`leanprover/lean4:v4.33.0` if it is not present. Verify the selected version from
the repository root:

```sh
cd consensus/spec/lean
lean --version
lake --version
```

The `lean --version` output must report version 4.33.0.

## Check the proofs

Run these commands from the repository root:

```sh
cd consensus/spec/lean
lake build
```

The build must complete without an error. The proof source must not contain an
unchecked placeholder:

```sh
rg -n '\b(sorry|admit|axiom)\b' Mysticeti.lean Mysticeti
```

An empty result is correct.

Check that each assumption identifier is defined and referenced:

```sh
bash consensus/spec/check-assumption-ledger.sh
```

Run this command from the repository root. A successful result reports the number
of checked identifiers.

## Current Rust mapping

The current `consensus/` code wires `FlexCommitter` into `Core` when v3 is enabled.
The leader-rule and local commit-progress mapping use that code directly.

The current proposer creates `BlockV1` or `BlockV2`; it does not create `BlockV3`.
The current tree uses `CommitFinalizer`; it does not contain `CommitFinalizerV3`.
Therefore, the Lean v3 transaction cutoff and transaction-finalization theorems are
protocol-model results. Their Rust mapping is not implemented in the current tree.

Read [the assumption ledger](docs/ASSUMPTIONS.md) and
[the proof scope](docs/PROOF_SCOPE.md) before you use a theorem as a protocol claim.
Use the
[periodic Lean-to-Rust refinement review](docs/ASSUMPTIONS.md#periodic-lean-to-rust-refinement-review)
after a relevant Rust change.
Read [the implementation gaps](docs/IMPLEMENTATION_GAPS.md) before v3 activation.
Read [the commit progress recovery design](design/commit_progress_recovery.md)
before you change threshold-clock advancement or recovery block production.
