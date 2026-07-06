#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""Dual-execution replay orchestration.

Wraps the mechanical steps of:
  1. checkout base commit
  2. generate execution-layer cut
  3. checkout tip commit (preserving the cut)
  4. inject dual-exec instrumentation into latest.rs and gas types
  5. cargo check
  6. commit

The `run` subcommand does all of the above. Each step is also exposed as its own
subcommand so a higher-level driver (e.g. the dual-replay skill) can resume after
a failure. Distinct exit codes per failed step tell the driver where to pick up.

Invocation:

    python3 scripts/dual_replay.py run --base <sha> --tip <sha> [opts...]

See `--help` for the full flag list.
"""

import argparse
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass, fields
from pathlib import Path
from string import Template
from typing import Optional

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

STATE_FILENAME = ".dual-replay.json"
VALID_DIFF_MODES = ("strict", "status-only", "gas-only", "status-and-gas")
MARKER = "// DUAL_REPLAY_INJECTED"

LATEST_RS = "sui-execution/src/latest.rs"
GAS_RS = "crates/sui-types/src/gas.rs"
GAS_V2_RS = "crates/sui-types/src/gas_model/gas_v2.rs"
GAS_TABLES_RS = "crates/sui-types/src/gas_model/tables.rs"
EXECUTION_LAYER = "scripts/execution_layer.py"

# Exit codes by failing step. The driver uses these to dispatch recovery.
EXIT_VALIDATION = 5
EXIT_CUT = 1
EXIT_INJECT = 2
EXIT_BUILD = 3
EXIT_COMMIT = 4


# ---------------------------------------------------------------------------
# Rust templates (parameterized with string.Template)
# ---------------------------------------------------------------------------
#
# Variables: $cut_pkg, $output_dir, $gas_tolerance, $diff_body, $marker.
# Note `$$` escapes a literal `$` (none currently needed; kept in mind).

TPL_EXECUTOR_STRUCT = """\
pub(crate) struct Executor(
    Arc<MoveRuntime>,
    Arc<move_vm_runtime_${cut_pkg}::runtime::MoveRuntime>,
);"""

TPL_EXECUTOR_IMPL = """\
impl Executor {
    pub(crate) fn new(protocol_config: &ProtocolConfig, silent: bool) -> Result<Self, SuiError> {
        let tip_runtime = Arc::new(new_move_runtime(
            all_natives(silent, protocol_config),
            protocol_config,
        )?);
        let base_runtime = Arc::new(sui_adapter_${cut_pkg}::adapter::new_move_runtime(
            sui_move_natives_${cut_pkg}::all_natives(silent, protocol_config),
            protocol_config,
        )?);
        Ok(Executor(tip_runtime, base_runtime))
    }
}"""

TPL_FN_NORMAL_BODY = """\
        ${marker}
        let tip_start = std::time::Instant::now();
        let (tip_store, tip_gas_status, tip_effects, _tip_timings, _tip_result) =
            execute_transaction_to_effects::<execution_mode::Normal>(
                store,
                input_objects.clone(),
                gas.clone(),
                gas_status.clone(),
                transaction_kind.clone(),
                rewritten_inputs.clone(),
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics.clone(),
                enable_expensive_checks,
                execution_params.clone(),
                &mut None,
            );
        let tip_ns = tip_start.elapsed().as_nanos() as u64;
        let base_start = std::time::Instant::now();
        let base = {
            use sui_adapter_${cut_pkg} as base_adapter;
            base_adapter::execution_engine::execute_transaction_to_effects::<
                base_adapter::execution_mode::Normal,
            >(
                store,
                input_objects,
                gas,
                gas_status,
                transaction_kind,
                rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.1,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
                trace_builder_opt,
            )
        };
        let base_ns = base_start.elapsed().as_nanos() as u64;
        self::latest_dual_replay::compare_dual_replay(
            (&base.0, &base.1, &base.2),
            (&tip_store, &tip_gas_status, &tip_effects),
            transaction_digest,
            base_ns,
            tip_ns,
        );
        if let Err(error) = &base.4 {
            log_execution_error(transaction_digest, error);
        }
        base
"""

TPL_FN_NORMAL_WITH_ERR_BODY = """\
        ${marker}
        let tip_start = std::time::Instant::now();
        let (tip_store, tip_gas_status, tip_effects, _tip_timings, _tip_result) =
            execute_transaction_to_effects::<execution_mode::Normal<ExecutionError>>(
                store,
                input_objects.clone(),
                gas.clone(),
                gas_status.clone(),
                transaction_kind.clone(),
                rewritten_inputs.clone(),
                transaction_signer,
                transaction_digest,
                &self.0,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics.clone(),
                enable_expensive_checks,
                execution_params.clone(),
                &mut None,
            );
        let tip_ns = tip_start.elapsed().as_nanos() as u64;
        let base_start = std::time::Instant::now();
        let base = {
            use sui_adapter_${cut_pkg} as base_adapter;
            base_adapter::execution_engine::execute_transaction_to_effects::<
                base_adapter::execution_mode::Normal<ExecutionError>,
            >(
                store,
                input_objects,
                gas,
                gas_status,
                transaction_kind,
                rewritten_inputs,
                transaction_signer,
                transaction_digest,
                &self.1,
                epoch_id,
                epoch_timestamp_ms,
                protocol_config,
                metrics,
                enable_expensive_checks,
                execution_params,
                trace_builder_opt,
            )
        };
        let base_ns = base_start.elapsed().as_nanos() as u64;
        self::latest_dual_replay::compare_dual_replay(
            (&base.0, &base.1, &base.2),
            (&tip_store, &tip_gas_status, &tip_effects),
            transaction_digest,
            base_ns,
            tip_ns,
        );
        if let Err(error) = &base.4 {
            log_execution_error(transaction_digest, error);
        }
        base
"""

TPL_DUAL_REPLAY_MODULE = """\
// ${marker}
mod latest_dual_replay {
    use std::fs::{File, OpenOptions};
    use std::io::{BufWriter, Write};
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::{SuiGasStatus, SuiGasStatusAPI};
    use sui_types::inner_temporary_store::InnerTemporaryStore;

    const OUTPUT_DIR: &str = "${output_dir}";
    const GAS_TOLERANCE_PCT: f64 = ${gas_tolerance}_f64;
    const TIMINGS_FILE: &str = "${timings_file}";
    const TIMINGS_FLUSH_EVERY: usize = 500;

    type View<'a> = (&'a InnerTemporaryStore, &'a SuiGasStatus, &'a TransactionEffects);

    pub(super) fn compare_dual_replay(
        base: View<'_>,
        tip: View<'_>,
        digest: TransactionDigest,
        base_ns: u64,
        tip_ns: u64,
    ) {
        let (_, base_gas, base_effects) = base;
        let (_, tip_gas, tip_effects) = tip;
        let base_gas_used = base_gas.gas_used();
        let tip_gas_used = tip_gas.gas_used();
        let status_match = matches!(
            (base_effects.status(), tip_effects.status()),
            (ExecutionStatus::Success, ExecutionStatus::Success)
                | (ExecutionStatus::Failure { .. }, ExecutionStatus::Failure { .. })
        );
        record_timing(digest, base_ns, tip_ns, base_gas_used, tip_gas_used, status_match);
        let differs = {
${diff_body}
        };
        if differs {
            report_diff(base_effects, tip_effects, digest);
        }
    }

    struct TimingsSink {
        writer: BufWriter<File>,
        pending: usize,
    }

    static TIMINGS: OnceLock<Mutex<TimingsSink>> = OnceLock::new();

    fn timings() -> &'static Mutex<TimingsSink> {
        TIMINGS.get_or_init(|| {
            let path = Path::new(TIMINGS_FILE);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .expect("dual-replay: failed to create timings dir");
                }
            }
            let is_new = !path.exists();
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("dual-replay: failed to open timings file");
            if is_new {
                f.write_all(b"digest,base_ns,tip_ns,base_gas,tip_gas,status_match\n")
                    .expect("dual-replay: failed to write timings header");
            }
            Mutex::new(TimingsSink {
                writer: BufWriter::new(f),
                pending: 0,
            })
        })
    }

    fn record_timing(
        digest: TransactionDigest,
        base_ns: u64,
        tip_ns: u64,
        base_gas: u64,
        tip_gas: u64,
        status_match: bool,
    ) {
        let mut guard = timings()
            .lock()
            .expect("dual-replay: timings mutex poisoned");
        writeln!(
            guard.writer,
            "{},{},{},{},{},{}",
            digest, base_ns, tip_ns, base_gas, tip_gas, status_match as u8,
        )
        .expect("dual-replay: failed to write timings row");
        guard.pending += 1;
        if guard.pending >= TIMINGS_FLUSH_EVERY {
            guard
                .writer
                .flush()
                .expect("dual-replay: failed to flush timings");
            guard.pending = 0;
        }
    }

    fn gas_within_tolerance(base_gas: u64, tip_gas: u64) -> bool {
        if GAS_TOLERANCE_PCT <= 0.0 {
            return base_gas == tip_gas;
        }
        let base = base_gas as f64;
        let tip = tip_gas as f64;
        let delta = (base - tip).abs();
        let denom = base.max(1.0);
        (delta / denom) * 100.0 <= GAS_TOLERANCE_PCT
    }

    fn report_diff(
        base_effects: &TransactionEffects,
        tip_effects: &TransactionEffects,
        digest: TransactionDigest,
    ) {
        tracing::warn!(%digest, "dual-replay: effects differ");
        std::fs::create_dir_all(Path::new(OUTPUT_DIR))
            .expect("dual-replay: failed to create output dir");
        let base_path = format!("{}/{}.base.json", OUTPUT_DIR, digest);
        let tip_path = format!("{}/{}.tip.json", OUTPUT_DIR, digest);
        let base_json = serde_json::to_string_pretty(base_effects)
            .expect("dual-replay: failed to serialize base effects");
        let tip_json = serde_json::to_string_pretty(tip_effects)
            .expect("dual-replay: failed to serialize tip effects");
        std::fs::write(&base_path, base_json)
            .expect("dual-replay: failed to write base effects");
        std::fs::write(&tip_path, tip_json)
            .expect("dual-replay: failed to write tip effects");
    }
}
"""

# Diff bodies: each is the contents of a `let differs = { ... };` block. They
# evaluate to `true` if base/tip differ in the dimension that mode cares about.

TPL_DIFF_BODIES = {
    "strict": """\
            let status_differs = base_effects.status() != tip_effects.status();
            let gas_differs = !gas_within_tolerance(base_gas.gas_used(), tip_gas.gas_used());
            let shape_differs = base_effects != tip_effects;
            status_differs || gas_differs || shape_differs
""",
    "status-only": """\
            let _ = (base_gas, tip_gas);
            match (base_effects.status(), tip_effects.status()) {
                (ExecutionStatus::Success, ExecutionStatus::Success) => false,
                (ExecutionStatus::Failure { .. }, ExecutionStatus::Failure { .. }) => false,
                _ => true,
            }
""",
    "gas-only": """\
            let _ = (base_effects, tip_effects);
            !gas_within_tolerance(base_gas.gas_used(), tip_gas.gas_used())
""",
    "status-and-gas": """\
            let status_differs = match (base_effects.status(), tip_effects.status()) {
                (ExecutionStatus::Success, ExecutionStatus::Success) => false,
                (ExecutionStatus::Failure { .. }, ExecutionStatus::Failure { .. }) => false,
                _ => true,
            };
            let gas_differs = !gas_within_tolerance(base_gas.gas_used(), tip_gas.gas_used());
            status_differs || gas_differs
""",
}


# ---------------------------------------------------------------------------
# State (persisted run config so recovery subcommands don't need flags re-passed)
# ---------------------------------------------------------------------------


@dataclass
class State:
    base: str = ""
    tip: str = ""
    cut_name: str = "replay_cut"
    diff_mode: str = "strict"
    gas_tolerance: float = 0.0
    output_dir: str = "target/dual-replay"
    timings_file: str = ""

    @classmethod
    def load(cls, repo: Path) -> "State":
        path = repo / STATE_FILENAME
        if not path.exists():
            return cls()
        data = json.loads(path.read_text())
        known = {f.name for f in fields(cls)}
        return cls(**{k: v for k, v in data.items() if k in known})

    def save(self, repo: Path) -> None:
        (repo / STATE_FILENAME).write_text(json.dumps(asdict(self), indent=2) + "\n")

    def merge_cli(self, **overrides: object) -> None:
        for k, v in overrides.items():
            if v is not None and hasattr(self, k):
                setattr(self, k, v)

    def validate(self) -> None:
        if not self.base:
            raise ValueError("base SHA is required (pass --base or run `cut` first)")
        if not self.tip:
            raise ValueError("tip SHA is required (pass --tip or run `cut` first)")
        if self.diff_mode not in VALID_DIFF_MODES:
            raise ValueError(
                f"diff-mode must be one of {VALID_DIFF_MODES}, got {self.diff_mode!r}"
            )
        if self.gas_tolerance < 0:
            raise ValueError("gas-tolerance must be non-negative")


# ---------------------------------------------------------------------------
# Git
# ---------------------------------------------------------------------------


class GitError(RuntimeError):
    pass


def _git(args: list[str], cwd: Path) -> str:
    r = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    if r.returncode != 0:
        raise GitError(
            f"`{' '.join(args)}` failed (exit {r.returncode}):\n{r.stderr.strip()}"
        )
    return r.stdout


def git_repo_root(start: Path) -> Path:
    return Path(_git(["git", "rev-parse", "--show-toplevel"], cwd=start).strip())


def git_resolve_sha(repo: Path, ref: str) -> str:
    return _git(["git", "rev-parse", "--verify", f"{ref}^{{commit}}"], cwd=repo).strip()


def git_short_sha(repo: Path, sha: str) -> str:
    return _git(["git", "rev-parse", "--short", sha], cwd=repo).strip()


def git_working_tree_dirty(repo: Path) -> bool:
    return bool(_git(["git", "status", "--porcelain"], cwd=repo).strip())


def git_checkout(repo: Path, sha: str) -> None:
    _git(["git", "checkout", sha], cwd=repo)


def git_merge_checkout(repo: Path, sha: str) -> None:
    """`git checkout -m` — preserves locally added files (the generated cut) by
    3-way merging instead of refusing on tracked workspace changes."""
    _git(["git", "checkout", "-m", sha], cwd=repo)


def git_add_paths(repo: Path, paths: list[str]) -> None:
    if paths:
        _git(["git", "add", "--"] + paths, cwd=repo)


def git_add_updated(repo: Path) -> None:
    _git(["git", "add", "-u"], cwd=repo)


def git_commit(repo: Path, message: str) -> str:
    _git(["git", "commit", "-m", message], cwd=repo)
    return _git(["git", "rev-parse", "HEAD"], cwd=repo).strip()


# ---------------------------------------------------------------------------
# Cut (step 1: checkout base, generate cut, checkout tip)
# ---------------------------------------------------------------------------


def _exec_layer(repo: Path, args: list[str]) -> str:
    cmd = [sys.executable, EXECUTION_LAYER] + args
    r = subprocess.run(cmd, cwd=repo, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(
            f"`{' '.join(cmd)}` failed (exit {r.returncode}):\n"
            f"stdout:\n{r.stdout}\nstderr:\n{r.stderr}"
        )
    return r.stdout


def do_cut(repo: Path, state: State, *, force_dirty: bool) -> None:
    state.base = git_resolve_sha(repo, state.base)
    state.tip = git_resolve_sha(repo, state.tip)
    if state.base == state.tip:
        raise RuntimeError("base and tip resolve to the same commit")
    if git_working_tree_dirty(repo) and not force_dirty:
        raise RuntimeError(
            "working tree is dirty; commit/stash changes or pass --force-dirty"
        )
    git_checkout(repo, state.base)
    _exec_layer(repo, ["cut", "--dry-run", state.cut_name])
    _exec_layer(repo, ["cut", state.cut_name])
    git_merge_checkout(repo, state.tip)


# ---------------------------------------------------------------------------
# Inject (step 2: rewrite latest.rs and gas types)
# ---------------------------------------------------------------------------


class InjectError(RuntimeError):
    pass


RE_EXECUTOR_STRUCT = re.compile(r"pub\(crate\)\s+struct\s+Executor\b")
RE_IMPL_EXECUTOR = re.compile(r"\bimpl\s+Executor\s*\{")
RE_FN_NORMAL = re.compile(r"fn\s+execute_transaction_to_effects\s*\(", re.MULTILINE)
RE_FN_NORMAL_WITH_ERR = re.compile(
    r"fn\s+execute_transaction_to_effects_and_execution_error\s*\(", re.MULTILINE
)


def do_inject(repo: Path, state: State) -> None:
    _inject_latest_rs(repo, state)
    _ensure_gas_clone(repo)
    _ensure_sui_execution_serde_json_dep(repo)


def _inject_latest_rs(repo: Path, state: State) -> None:
    path = repo / LATEST_RS
    src = path.read_text()
    if MARKER in src:
        return
    cut_pkg = state.cut_name.replace("-", "_")

    # 1. Executor struct + impl new.
    src = _replace_executor_overlay(src, cut_pkg)

    # 2. Body of `execute_transaction_to_effects`. The shorter name is a prefix of
    # the `_and_execution_error` variant, so skip matches that extend further.
    src = _replace_fn_body(
        src,
        anchor=RE_FN_NORMAL,
        body=Template(TPL_FN_NORMAL_BODY).substitute(
            cut_pkg=cut_pkg, marker=MARKER
        ),
        anchor_label="fn execute_transaction_to_effects(",
        excluded_name_prefix="execute_transaction_to_effects_and_execution_error",
    )

    # 3. Body of `execute_transaction_to_effects_and_execution_error`.
    src = _replace_fn_body(
        src,
        anchor=RE_FN_NORMAL_WITH_ERR,
        body=Template(TPL_FN_NORMAL_WITH_ERR_BODY).substitute(
            cut_pkg=cut_pkg, marker=MARKER
        ),
        anchor_label="fn execute_transaction_to_effects_and_execution_error(",
    )

    # 4. Append the dual-replay helper module.
    diff_body = TPL_DIFF_BODIES[state.diff_mode].rstrip()
    timings_file = state.timings_file or f"{state.output_dir.rstrip('/')}/timings.csv"
    module = Template(TPL_DUAL_REPLAY_MODULE).substitute(
        output_dir=state.output_dir,
        gas_tolerance=state.gas_tolerance,
        diff_body=diff_body,
        marker=MARKER,
        timings_file=timings_file,
    )
    src = src.rstrip() + "\n\n" + module + "\n"
    path.write_text(src)


def _replace_executor_overlay(src: str, cut_pkg: str) -> str:
    """Replace the `Executor` struct declaration and the `impl Executor` block,
    independently — there can be other items (e.g. `struct Verifier`) between them
    that must be preserved."""
    m = RE_EXECUTOR_STRUCT.search(src)
    if not m:
        raise InjectError(
            f"anchor `pub(crate) struct Executor` not found in {LATEST_RS}"
        )
    struct_end = _end_of_struct_decl(src, m.end())
    new_struct = Template(TPL_EXECUTOR_STRUCT).substitute(cut_pkg=cut_pkg)
    src = src[: m.start()] + new_struct + src[struct_end:]

    impl_match = RE_IMPL_EXECUTOR.search(src)
    if not impl_match:
        raise InjectError(f"anchor `impl Executor {{` not found in {LATEST_RS}")
    impl_open = src.find("{", impl_match.start())
    impl_close = _matching(src, impl_open, "{", "}")
    if impl_close < 0:
        raise InjectError(f"unbalanced braces on `impl Executor` in {LATEST_RS}")
    new_impl = Template(TPL_EXECUTOR_IMPL).substitute(cut_pkg=cut_pkg)
    return src[: impl_match.start()] + new_impl + src[impl_close + 1 :]


def _end_of_struct_decl(src: str, after_name: int) -> int:
    """Given an index just after `struct Executor`, return the index right after
    the end of the declaration: `);` for a tuple struct, `}` for a named struct,
    or `;` for a unit struct. Skips generics if present."""
    i = after_name
    while i < len(src) and src[i] in " \t\n":
        i += 1
    if i < len(src) and src[i] == "<":
        gen_close = _matching(src, i, "<", ">")
        if gen_close < 0:
            raise InjectError("unbalanced generics on `struct Executor`")
        i = gen_close + 1
    while i < len(src) and src[i] in " \t\n":
        i += 1
    if i >= len(src):
        raise InjectError("unexpected end of file in `struct Executor`")
    if src[i] == "(":
        paren_close = _matching(src, i, "(", ")")
        j = paren_close + 1
        while j < len(src) and src[j] in " \t\n":
            j += 1
        if j < len(src) and src[j] == ";":
            return j + 1
        return j
    if src[i] == "{":
        body_close = _matching(src, i, "{", "}")
        return body_close + 1
    if src[i] == ";":
        return i + 1
    raise InjectError(
        f"unrecognized struct form after `struct Executor`: {src[i:i+20]!r}"
    )


def _replace_fn_body(
    src: str,
    *,
    anchor: re.Pattern[str],
    body: str,
    anchor_label: str,
    excluded_name_prefix: Optional[str] = None,
) -> str:
    start = 0
    while True:
        m = anchor.search(src, start)
        if not m:
            raise InjectError(f"anchor `{anchor_label}` not found in {LATEST_RS}")
        name_start = m.start() + len("fn ")
        name_end = src.find("(", name_start)
        name = src[name_start:name_end].strip()
        if excluded_name_prefix and name.startswith(excluded_name_prefix):
            start = m.end()
            continue
        break
    paren_open = src.find("(", m.end() - 1)
    paren_close = _matching(src, paren_open, "(", ")")
    body_open = src.find("{", paren_close)
    if body_open < 0:
        raise InjectError(f"could not find body `{{` for {anchor_label} in {LATEST_RS}")
    body_close = _matching(src, body_open, "{", "}")
    if body_close < 0:
        raise InjectError(f"unbalanced braces in {anchor_label} body in {LATEST_RS}")
    return src[: body_open + 1] + "\n" + body.rstrip() + "\n    " + src[body_close:]


def _matching(src: str, open_idx: int, open_c: str, close_c: str) -> int:
    """Brace/paren matcher that ignores `"..."`, `// ...`, and `/* ... */`.
    Not a full Rust lexer — sufficient for well-formed code we target."""
    if open_idx < 0 or src[open_idx] != open_c:
        return -1
    i = open_idx
    depth = 0
    in_str = in_line = in_block = False
    while i < len(src):
        c = src[i]
        nxt = src[i + 1] if i + 1 < len(src) else ""
        if in_line:
            if c == "\n":
                in_line = False
            i += 1
            continue
        if in_block:
            if c == "*" and nxt == "/":
                in_block = False
                i += 2
                continue
            i += 1
            continue
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
            continue
        if c == "/" and nxt == "/":
            in_line = True
            i += 2
            continue
        if c == "/" and nxt == "*":
            in_block = True
            i += 2
            continue
        if c == '"':
            in_str = True
            i += 1
            continue
        if c == open_c:
            depth += 1
        elif c == close_c:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


# The dual-exec body clones `SuiGasStatus` (gas.rs enum). That cascades through
# its field types — every nested type that participates in `Clone` must derive it.
# This list is the empirical minimum from running the pipeline against current
# HEAD; extend if a future tip introduces another field type that breaks.
GAS_CLONE_TARGETS = [
    (GAS_RS, "SuiGasStatus", "enum"),
    (GAS_V2_RS, "SuiGasStatus", "struct"),
    (GAS_V2_RS, "SuiCostTable", "struct"),
    (GAS_V2_RS, "ComputationBucket", "struct"),
    (GAS_TABLES_RS, "GasStatus", "struct"),
]


def _ensure_gas_clone(repo: Path) -> None:
    for relpath, name, kind in GAS_CLONE_TARGETS:
        _ensure_clone_derive(repo / relpath, name, kind)


def _ensure_clone_derive(path: Path, type_name: str, kind: str) -> None:
    src = path.read_text()
    # Visibility prefix is optional (matches `pub`, `pub(crate)`, or none).
    vis = r"(?:pub(?:\([^)]*\))?\s+)?"
    decl_re = re.compile(rf"{vis}{kind}\s+{re.escape(type_name)}\b", re.MULTILINE)
    derive_re = re.compile(
        rf"(#\[derive\(([^)]*)\)\])(\s*(?:#\[[^\]]*\]\s*)*)({vis}{kind}\s+{re.escape(type_name)}\b)",
        re.MULTILINE,
    )

    m = derive_re.search(src)
    if m:
        derives = [d.strip() for d in m.group(2).split(",") if d.strip()]
        if "Clone" in derives:
            return
        new_attr = f"#[derive({', '.join(derives + ['Clone'])})]"
        replacement = new_attr + m.group(3) + m.group(4)
        path.write_text(src[: m.start()] + replacement + src[m.end() :])
        return

    s = decl_re.search(src)
    if not s:
        raise InjectError(f"anchor `{kind} {type_name}` not found in {path}")
    line_start = src.rfind("\n", 0, s.start()) + 1
    indent_match = re.match(r"\s*", src[line_start : s.start()])
    indent = indent_match.group(0) if indent_match else ""
    path.write_text(src[:line_start] + f"{indent}#[derive(Clone)]\n" + src[line_start:])


# The injected `latest_dual_replay` helper module calls `serde_json::to_string_pretty`
# on `TransactionEffects`. `sui-execution` doesn't depend on `serde_json` by default
# at HEAD, so the cargo check fails until we add it. Idempotent.
RE_SUI_EXECUTION_DEPS_HEADER = re.compile(r"^\[dependencies\]\s*$", re.MULTILINE)
SUI_EXECUTION_CARGO_TOML = "sui-execution/Cargo.toml"


def _ensure_sui_execution_serde_json_dep(repo: Path) -> None:
    path = repo / SUI_EXECUTION_CARGO_TOML
    src = path.read_text()
    if re.search(r"^serde_json\b", src, re.MULTILINE):
        return
    m = RE_SUI_EXECUTION_DEPS_HEADER.search(src)
    if not m:
        raise InjectError(
            f"anchor `[dependencies]` not found in {SUI_EXECUTION_CARGO_TOML}"
        )
    insert_at = m.end()
    # `\n` after the header, then our line, then resume with the rest. The original
    # `\n` that ended the header line is already in src[m.end():].
    path.write_text(
        src[:insert_at] + "\nserde_json.workspace = true" + src[insert_at:]
    )


# ---------------------------------------------------------------------------
# Build & commit (steps 3 & 4)
# ---------------------------------------------------------------------------


def do_build(repo: Path) -> None:
    cmd = ["cargo", "check", "-p", "sui-execution", "-p", "sui-types"]
    r = subprocess.run(cmd, cwd=repo, capture_output=True, text=True)
    if r.returncode != 0:
        # Truncate to the last 200 stderr lines — rustc diagnostics land here.
        tail = "\n".join(r.stderr.splitlines()[-200:])
        raise RuntimeError(
            f"cargo check failed (exit {r.returncode}). Last 200 lines of stderr:\n{tail}"
        )


COMMIT_FIXED_PATHS = [
    LATEST_RS,
    GAS_RS,
    GAS_V2_RS,
    GAS_TABLES_RS,
    "sui-execution/Cargo.toml",
    "Cargo.toml",
    "external-crates/move/Cargo.toml",
]


def do_commit(repo: Path, state: State) -> str:
    paths = [p for p in COMMIT_FIXED_PATHS if (repo / p).exists()]
    # Per-cut-name paths created by `scripts/execution_layer.py cut`:
    #   - sui-execution/<cut>/             generated crate tree
    #   - sui-execution/src/<cut>.rs       module wiring file (untracked until staged)
    #   - external-crates/.../<cut>/       generated Move crate tree
    for p in (
        f"sui-execution/{state.cut_name}",
        f"sui-execution/src/{state.cut_name}.rs",
        f"external-crates/move/move-execution/{state.cut_name}",
    ):
        if (repo / p).exists():
            paths.append(p)
    git_add_paths(repo, paths)
    # Defensive: pick up any other tracked-file modifications introduced by injection
    # that we didn't enumerate (e.g. if a future template touches another file).
    git_add_updated(repo)

    base_short = git_short_sha(repo, state.base)
    tip_short = git_short_sha(repo, state.tip)
    subject = f"instrument dual execution replay from {base_short} to {tip_short}"
    message = (
        f"{subject}\n\n"
        f"base: {state.base}\n"
        f"tip: {state.tip}\n"
        f"cut: {state.cut_name}\n"
        f"diff-mode: {state.diff_mode}\n"
        f"gas-tolerance: {state.gas_tolerance}%\n"
        f"output-dir: {state.output_dir}\n\n"
    )
    return git_commit(repo, message)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _add_flags(p: argparse.ArgumentParser, *, base_required: bool) -> None:
    p.add_argument("--base", required=base_required, default=None)
    p.add_argument("--tip", required=base_required, default=None)
    p.add_argument("--cut-name", default=None)
    p.add_argument("--diff-mode", choices=VALID_DIFF_MODES, default=None)
    p.add_argument("--gas-tolerance", type=float, default=None)
    p.add_argument("--output-dir", default=None)
    p.add_argument("--timings-file", default=None)
    p.add_argument("--force-dirty", action="store_true")


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="dual_replay.py")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_run = sub.add_parser("run", help="happy-path: cut → inject → build → commit")
    _add_flags(p_run, base_required=True)
    p_cut = sub.add_parser("cut", help="checkout base, generate cut, checkout tip")
    _add_flags(p_cut, base_required=False)
    p_inject = sub.add_parser("inject", help="rewrite latest.rs and gas types")
    _add_flags(p_inject, base_required=False)
    sub.add_parser("build", help="cargo check -p sui-execution -p sui-types")
    sub.add_parser("commit", help="stage changes and commit")
    return parser


def _load_state(repo: Path, args: argparse.Namespace) -> State:
    s = State.load(repo)
    s.merge_cli(
        base=getattr(args, "base", None),
        tip=getattr(args, "tip", None),
        cut_name=getattr(args, "cut_name", None),
        diff_mode=getattr(args, "diff_mode", None),
        gas_tolerance=getattr(args, "gas_tolerance", None),
        output_dir=getattr(args, "output_dir", None),
        timings_file=getattr(args, "timings_file", None),
    )
    return s


def _fail(step: str, msg: str, code: int) -> int:
    print(f"[dual-replay] {step} failed: {msg}", file=sys.stderr)
    return code


def cmd_cut(repo: Path, state: State, force_dirty: bool) -> int:
    try:
        do_cut(repo, state, force_dirty=force_dirty)
    except Exception as e:
        return _fail("cut", str(e), EXIT_CUT)
    state.save(repo)
    return 0


def cmd_inject(repo: Path, state: State) -> int:
    try:
        state.validate()
        do_inject(repo, state)
    except Exception as e:
        return _fail("inject", str(e), EXIT_INJECT)
    state.save(repo)
    return 0


def cmd_build(repo: Path) -> int:
    try:
        do_build(repo)
    except Exception as e:
        return _fail("build", str(e), EXIT_BUILD)
    return 0


def cmd_commit(repo: Path, state: State) -> int:
    try:
        state.validate()
        sha = do_commit(repo, state)
    except Exception as e:
        return _fail("commit", str(e), EXIT_COMMIT)
    print(sha)
    return 0


def cmd_run(repo: Path, state: State, force_dirty: bool) -> int:
    rc = cmd_cut(repo, state, force_dirty=force_dirty)
    if rc:
        return rc
    rc = cmd_inject(repo, state)
    if rc:
        return rc
    rc = cmd_build(repo)
    if rc:
        return rc
    return cmd_commit(repo, state)


def main(argv: Optional[list[str]] = None) -> int:
    args = _build_parser().parse_args(argv)
    repo = git_repo_root(Path.cwd())
    state = _load_state(repo, args)
    if args.cmd == "run":
        return cmd_run(repo, state, force_dirty=args.force_dirty)
    if args.cmd == "cut":
        return cmd_cut(repo, state, force_dirty=args.force_dirty)
    if args.cmd == "inject":
        return cmd_inject(repo, state)
    if args.cmd == "build":
        return cmd_build(repo)
    if args.cmd == "commit":
        return cmd_commit(repo, state)
    return _fail("dispatch", f"unknown subcommand {args.cmd!r}", EXIT_VALIDATION)


if __name__ == "__main__":
    sys.exit(main())
