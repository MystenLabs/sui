<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 formal specification

This directory contains a Lean 4 model of Mysticeti v3. The model proves that
leader and transaction decisions do not conflict when the listed safety
conditions hold. It also proves conditional progress for consensus, commit
recovery, and transaction decisions.

The proofs do not inspect the product source. Some recovery and transaction
behavior is not implemented. Do not apply a theorem to the product until its
assumptions and implementation gaps are closed.

## Documents

- [Proof scope](docs/PROOF_SCOPE.md): proved results, limits, and open boundaries.
- [Assumption ledger](docs/ASSUMPTIONS.md): conditions and their current status.
- [Implementation gaps](docs/IMPLEMENTATION_GAPS.md): required product work.
- [Commit progress recovery](design/commit_progress_recovery.md): the proposed
  recovery policy.
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
lake build
rg -n '\b(sorry|admit|axiom)\b' Mysticeti.lean Mysticeti
cd ../../..
bash consensus/spec/check-assumption-ledger.sh
```

The build and ledger check must succeed. The search command must return no
results.
