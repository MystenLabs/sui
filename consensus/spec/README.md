<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 formal specification

This directory contains a Lean 4 model of Mysticeti v3. The model proves that
leader and transaction decisions do not conflict when the listed safety
conditions hold. It also contains liveness lemmas and stage-composition results.
The complete network DAG progress, unbounded commit progress, and pointwise
catch-up proof from fundamental network and single-validator rules is still
open.

The proofs do not inspect the product source. Some recovery and transaction
behavior is not implemented. Do not apply a theorem to the product until all
conditions and gaps for that theorem are closed.

## Documents

- [Safety and liveness properties](docs/SAFETY_AND_LIVENESS.md): the proof
  contract in plain language.
- [Proof scope](docs/PROOF_SCOPE.md): proved results, limits, and open boundaries.
- [Assumption ledger](docs/ASSUMPTIONS.md): conditions and their current status.
- [Assumption evidence](docs/ASSUMPTION_EVIDENCE.md): reviewed Rust and Lean
  evidence, limits, revisions, and focused revalidation triggers.
- [Implementation gaps](docs/IMPLEMENTATION_GAPS.md): required product work.
- [Commit progress recovery](design/commit_progress_recovery.md): the proposed
  recovery policy.
- [Liveness proof plan](design/liveness_proof_plan.md): the theorem boundary and
  proof plan for block production and commit progress.
- [Implementation review](docs/ASSUMPTIONS.md#periodic-lean-to-rust-refinement-review):
  the checklist to use after a related product change.

## Install Lean

The project pins Lean 4.33.0 in `lean/lean-toolchain`. Elan installs Lean and
Lake.

On macOS or Linux, use the installer from the
[Lean install page](https://lean-lang.org/install/):

```sh
curl https://elan.lean-lang.org/elan-init.sh -sSf | sh
source "$HOME/.elan/env"
elan toolchain install leanprover/lean4:v4.33.0
```

On Windows, install Elan from the
[Elan release page](https://github.com/leanprover/elan/releases). Then run:

```powershell
elan toolchain install leanprover/lean4:v4.33.0
```

Start a new shell after installation. From `consensus/spec/lean`, verify the
tools:

```sh
elan --version
lean --version
lake --version
```

Lean must report version 4.33.0.

## Check the specification

From the repository root, run:

```sh
cd consensus/spec/lean
lake exe cache get
lake build
rg -n '\b(sorry|admit|axiom)\b' Mysticeti.lean Mysticeti
cd ../../..
bash consensus/spec/check-assumption-ledger.sh
```

The specification depends on Mathlib. `lake exe cache get` downloads the
prebuilt Mathlib files. Without it, the first build compiles Mathlib from
source, which takes hours.

The build and ledger check must succeed. The search command must return no
results.
