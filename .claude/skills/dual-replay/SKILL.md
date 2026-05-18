# Dual Execution Replay

Instruments the `latest` execution path to run every transaction twice — once with a base executor (built from an execution-layer cut taken at `<base-sha>`) and once with the tip executor at `<tip-sha>` — and diffs the two outputs. The base result is committed; the tip result is for comparison only.

Most of the work is handled by `scripts/dual_replay/`. This skill is the contract for invoking that script and the playbook for when its happy path fails.

## Usage

```
/dual-replay <base-sha> <tip-sha> [diff-mode] [cut-name]
```

- `diff-mode` (optional, default `strict`): one of `strict`, `status-only`, `gas-only`, `status-and-gas`.
- `cut-name` (optional, default `replay_cut`).

Additional knobs (pass through if the user supplied them):

- `--gas-tolerance <percent>`: tolerated gas delta as a percentage (default `0`, i.e. exact equality). Only meaningful for diff modes that compare gas.
- `--output-dir <path>`: where effects-diff artifacts go (default `target/dual-replay/`).
- `--timings-file <path>`: where per-tx execution timings are appended as CSV (default `<output-dir>/timings.csv`).

## Timings output

Every transaction's base and tip execution is timed and appended as one CSV row to `--timings-file` (default `<output-dir>/timings.csv`). Columns: `digest,base_ns,tip_ns,base_gas,tip_gas,status_match`. Writes are buffered and flushed every 500 rows. The file is opened in append mode, so resumed runs accumulate into the same file — delete it between runs if a clean dataset is needed.

If either SHA is missing or does not resolve to a commit, ask the user for clarification. Do not prompt for other arguments — fall back to defaults.

## Happy path

Run the script's `run` subcommand. It does everything end-to-end and creates a commit on the current branch:

```bash
python3 scripts/dual_replay.py run \
  --base <base-sha> --tip <tip-sha> \
  --cut-name <cut-name> \
  --diff-mode <diff-mode> \
  [--gas-tolerance <percent>] \
  [--output-dir <path>]
```

On success, report the commit SHA produced (`git rev-parse HEAD`) and stop. **Do not open a PR.**

The script never prompts. If you need to override the dirty-tree refusal because the user explicitly said so, pass `--force-dirty`.

## Recovery playbook

If `run` exits non-zero, the script prints which subcommand failed. Drive the remaining steps individually — they read persisted state from `.dual-replay.json` so you don't need to re-pass flags:

```bash
python3 scripts/dual_replay.py cut    --base <sha> --tip <sha> --cut-name <name>
python3 scripts/dual_replay.py inject
python3 scripts/dual_replay.py build
python3 scripts/dual_replay.py commit
```

### `cut` failed

Usually environmental: dirty tree, unresolvable SHA, or `scripts/execution_layer.py cut` itself errored. Read the script's stderr, surface it to the user, and stop. Do not attempt code edits — the working tree may be on an unexpected commit.

### `inject` failed (anchor not found)

The script reports the file and the regex it tried. The tip has likely renamed or moved a symbol since the templates were written. Do not refresh the templates speculatively. Instead:

1. Read the named file and locate the equivalent symbol at tip.
2. Apply the dual-exec changes manually for this run, following the shape in the `TPL_*` template constants in `scripts/dual_replay.py`. Substitute `${cut_pkg}` → the underscored cut name (e.g. `replay_cut` → `sui_adapter_replay_cut`).
3. Skip the inject step (state is preserved) and continue: `python3 scripts/dual_replay.py build`, then `commit`.
4. If the same anchor breaks repeatedly across invocations, the regex in `scripts/dual_replay.py` should be updated — but only after confirming the new symbol name is stable.

### `build` failed

`cargo check -p sui-execution -p sui-types` failed. The script already handles the common scaffolding (Clone derives on the gas types it knows about, the `serde_json` dep on `sui-execution`). What's left is genuinely case-by-case. Read the captured stderr and apply the smallest fix that compiles:

- **Generated cut references an API that no longer exists at tip.** Apply the smallest compatibility patch. Prefer adapting code *inside the cut* (it's a snapshot, you're free to adjust it) over changing shared code at tip. Sometimes restoring a tiny helper in shared code is cleaner — judge per case.
- **Cascading `Clone` derives.** The script derives `Clone` on `SuiGasStatus` (gas.rs), `gas_v2::SuiGasStatus`, `gas_v2::SuiCostTable`, `gas_v2::ComputationBucket`, and `tables::GasStatus`. If a new field type in any of these (or a transitively-reached type) is not `Clone`, add `#[derive(Clone)]` to it — and if this becomes a recurring fix, add the type to `GAS_CLONE_TARGETS` in `scripts/dual_replay.py` so the script handles it directly.
- **Missing gas-model setter.** If the template calls `set_gas_model_version` on `SuiGasStatus` and the method doesn't exist, add a narrow one. Keep it minimal.
- **Cut crates fail formatting/metadata checks because they think they're part of the nested Move workspace.** Update `external-crates/move/Cargo.toml` so the generated cut crates are either members or explicitly excluded — keep the metadata consistent. Don't skip formatting.

After each fix, re-run `python3 scripts/dual_replay.py build`. When it passes, run `commit`.

### `commit` failed

Almost always pre-commit hook failure. Read the hook output, fix the cited issue (often `cargo fmt` or `cargo xclippy` complaints), and re-run `commit`. Never use `--no-verify`.

## Constraints

- Do not modify scope beyond what the dual-replay flow requires. If you notice unrelated issues, mention them but do not fix them in this commit.
- Do not reset or clean the working tree to "tidy up" after the cut. The generated cut crates must survive the checkout from base to tip; the script uses `git checkout -m` for exactly this reason.
- Do not double-run or diff non-normal execution paths (dev-inspect, genesis state update, verifier, layout resolver) unless the user explicitly asks for it. If a compatibility patch routes one of those paths through the cut just to compile, that's fine — note it.
- The final commit message should be: `instrument dual execution replay from <base-short-sha> to <tip-short-sha>`. The script produces this automatically.
- Do not push. Do not open a PR. The bot's caller does that separately.
