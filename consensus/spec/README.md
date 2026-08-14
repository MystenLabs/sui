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

The safety proof includes the Core garbage-collection boundary, the v3 sub-DAG
retention window, and the signed transaction vote cutoff. The transaction cutoff is
the maximum of the causal-history block-GC round and the transaction vote-tracker
GC round. The proof keeps live DAG evidence separate from the finalizer's buffered
committed-prefix evidence.

The conditional liveness proof also includes commit progress recovery. It derives
the recovery layer count from the v3 direct-vote offset and indirect depth. It does
not claim liveness for old leader blocks or transaction inclusion. Read the implementation
gaps before you treat the theorem as a Rust guarantee.

The recovery proof separates the validator set, the leader schedule, the round
leader selection, and each selected leader slot. Current v3 uses the full leader
schedule as the round leader selection in every pending leader round. The general
schedule and selection lemmas do not assume this equality. The v3 commit progress
recovery theorem does.

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

## Analysis baseline

The proof maps to a synthetic combined state of these pull requests:

- [PR 27505](https://github.com/MystenLabs/sui/pull/27505), through commit
  `ad78afa56828cdcf89008a3f134a4c6d5a08272d`.
- [PR 27655](https://github.com/MystenLabs/sui/pull/27655), through commit
  `f3b782d418029ba1c5367da6769c32dec5afd65a`.

The analysis applied the PR 27505 commits first. It then applied the PR 27655
commits. This order kept the FlexCommitter path and the transaction voting path in
one source tree.

This branch contains only the formal artifacts on top of `origin/main`. It does not
copy the PR commits. Use the listed PR heads, or a later main commit with equivalent
changes, when you check the proof-to-Rust mapping.

Read [the assumption ledger](docs/ASSUMPTIONS.md) and
[the proof scope](docs/PROOF_SCOPE.md) before you use a theorem as a protocol claim.
Read [the implementation gaps](docs/IMPLEMENTATION_GAPS.md) before v3 activation.
Read [the commit progress recovery design](design/commit_progress_recovery.md)
before you change threshold-clock advancement or recovery block production.
