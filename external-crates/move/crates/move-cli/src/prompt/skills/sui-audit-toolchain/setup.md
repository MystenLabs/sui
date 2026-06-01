# Setup — clean toolchain via `suiup` + fresh `move` build

Run once per session, then reuse `$MOVE_BIN` IF it was built in *this* `$AUDIT_WORK` from the
currently-confirmed `SUI_REF`. See `SKILL.md` for the **Isolation** rule, `SUI_REF`, and
`AUDIT_WORK`. **Never** reuse a binary or checkout from anywhere else.

## 1. Install / verify a clean `sui` via `suiup`

```sh
command -v suiup    # is suiup present?
cargo --version
git --version
```

**If `suiup` is missing → ASK THE USER for permission to install it** (outward-facing action; do
not install without consent). On approval, run the official installer:

```sh
curl -sSfL https://raw.githubusercontent.com/MystenLabs/suiup/main/install.sh | sh
# then ensure suiup's bin dir is on PATH for this session, per its install output
```

Install and switch to the `sui` CLI for the target network (match the network of the package being
audited — `testnet` or `mainnet`):

```sh
suiup install sui@<network>
suiup switch  sui@<network>
sui --version
```

**Provenance check** — required before continuing. `command -v sui` must resolve to a
`suiup`-managed path (typically under `~/.local/` or `~/.suiup/`), and must NOT contain `target/`
(which would indicate a binary built inside some local Sui checkout):

```sh
which sui
case "$(command -v sui)" in
  *target*) echo "REFUSE: 'sui' resolves inside a local build (target/). Re-do via suiup."; exit 2;;
esac
```

If the check fails, stop and tell the user: the audit needs a `suiup`-managed `sui`, not a binary
from a local checkout, to keep findings reproducible.

## 2. Confirm the ref WITH THE USER

> Do not skip this. State the default and ask before cloning/building:
> "I'll clone `MystenLabs/sui` at **`$SUI_REF`** and build the `move` decompiler (a multi-minute
> cargo build). Proceed, or use a different ref?"

Only continue once confirmed. **Reuse `$MOVE_BIN` only if it was built in this `$AUDIT_WORK` from
the currently-confirmed `SUI_REF`**; otherwise rebuild. Do not reuse a `move` binary from anywhere
else — not from `$PATH`, not from another `target/` dir, not from any `~/sui*` checkout.

## 3. Shallow-clone Sui at the pinned tag

```sh
mkdir -p "$AUDIT_WORK"
git clone --depth 1 --branch "$SUI_REF" \
  https://github.com/MystenLabs/sui.git "$AUDIT_WORK/sui"
```

If the tag is unknown, list available tags: `git ls-remote --tags https://github.com/MystenLabs/sui.git | grep sui_v`.
A branch (e.g. `main`) or commit also works with `--branch`/a follow-up `git checkout`.

## 4. Build only the Move CLI (not all of Sui)

The Move tooling is its own cargo workspace under `external-crates/move`, far lighter than the
full Sui build:

```sh
cd "$AUDIT_WORK/sui/external-crates/move"
cargo build --release -p move-cli --bin move
```

Resulting binary — **this is the ONLY `move` binary to use for the audit**:

```sh
MOVE_BIN="$AUDIT_WORK/sui/external-crates/move/target/release/move"
```

Ignore any `move` on `$PATH` or under any directory other than `$AUDIT_WORK`. Do not run
`which move`/`command -v move` to "find" one — always use the path above explicitly.

If a newer `SUI_REF` changes the workspace layout and the binary lands elsewhere, only search
*inside* `$AUDIT_WORK`:
`find "$AUDIT_WORK/sui" -path '*/target/release/move' -type f`.

## 5. Verify the toolchain

```sh
# Decompiler must be the freshly-built one inside AUDIT_WORK
printf '%s\n' "$MOVE_BIN"
test -x "$MOVE_BIN" || { echo "MOVE_BIN missing or not executable"; exit 2; }
case "$MOVE_BIN" in "$AUDIT_WORK"/*) : ;; *) echo "REFUSE: MOVE_BIN outside AUDIT_WORK"; exit 2;; esac
"$MOVE_BIN" decompile --help                # expect: "Decompile Move bytecode into Move source code"

# sui CLI must be the suiup-managed one (re-check the provenance from step 1)
which sui
case "$(command -v sui)" in *target*) echo "REFUSE: 'sui' inside a local build"; exit 2;; esac
sui move disassemble --help                 # expect the disassemble usage
```

All four succeeding means the toolchain is clean — proceed to `fetch-and-decompile.md`.

## Notes

- The build is the only slow step; the clone is shallow. Reuse `$MOVE_BIN` across packages
  **only within the same `$AUDIT_WORK` built from the same `SUI_REF`** — never elsewhere.
- `$AUDIT_WORK` is disposable. To reclaim space after an audit, delete it (you'll rebuild next
  time). Keep it to skip rebuilding.
- **Hard rule:** public sources only. The pinned `MystenLabs/sui` clone in `$AUDIT_WORK` and the
  `suiup`-managed `sui` are the only acceptable origins for analysis tooling. Any other local Sui
  checkout or binary is off-limits — see the Isolation block in `SKILL.md`.
