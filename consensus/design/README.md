<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 formal design

This directory contains a Lean 4 proof model for Mysticeti v3.

The model has two parts:

- `lean/` contains the machine-checked definitions and theorems.
- `docs/` contains the proof scope and the implementation gap report.

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
commands from `consensus/design/lean` to make Elan select the pinned Lean version.

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
cd consensus/design/lean
lean --version
lake --version
```

The `lean --version` output must report version 4.33.0.

## Check the proofs

Run these commands from the repository root:

```sh
cd consensus/design/lean
lake build
```

The build must complete without an error. The proof source must not contain an
unchecked placeholder:

```sh
rg -n '\b(sorry|admit|axiom)\b' Mysticeti.lean Mysticeti
```

An empty result is correct.

## Analysis baseline

The proof maps to a local combined state of these pull requests:

- [PR 27505](https://github.com/MystenLabs/sui/pull/27505), through commit
  `ad78afa56828cdcf89008a3f134a4c6d5a08272d`.
- [PR 27655](https://github.com/MystenLabs/sui/pull/27655), through commit
  `f3b782d418029ba1c5367da6769c32dec5afd65a`.

The local branch applies the PR 27505 commits first. It then applies the PR 27655
commits. This order keeps the FlexCommitter path and the transaction voting path in
one source tree.

Read [the proof scope](docs/PROOF_SCOPE.md) before you use a theorem as a protocol
claim. Read [the implementation gaps](docs/IMPLEMENTATION_GAPS.md) before v3
activation.
