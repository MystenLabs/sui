# Design: Basic-Block Stack-Aware Optimizer for Move Bytecode

**Status**: draft
**Author**: —
**Scope**: v1 — no new opcodes, no super-instructions. Output is valid Move
bytecode in the existing file format.

## Motivation

A bytecode profile over 10k mainnet transactions (103M dynamic instructions)
showed local-variable movement (`COPY_LOC` + `MOVE_LOC` + `ST_LOC`) accounts
for **~42% of executed instructions**. Static analysis of 1,857 modules
(542K static instructions) found:

| Redundant pattern | Static count | % of StLocs |
|---|---:|---:|
| `MoveLoc(x); StLoc(y)` pure rename | 7,356 | 14% |
| `StLoc; StLoc` (unpack prep) | 5,255 | 10% |
| `MoveLoc(x); Ret` tail return | 4,395 | — |
| `StLoc(x); CopyLoc(x)` store-then-reload | 3,976 | 7.6% |
| `CopyLoc(x); StLoc(y)` rename of copy | 1,038 | — |

Basic blocks are short (median 5 instructions), so per-block dispatch cost is
high and small optimizations compose well.

This doc describes a single pass that runs at JIT time, operates on one basic
block at a time, and eliminates as much of the above redundancy as is possible
**without introducing new opcodes**.

## Constraints

- **Output format**: same existing Move bytecode. The verifier must still
  accept it. No `Dup`, no `CallN`, no stack-peek.
- **No cross-block analysis**: everything is BB-local. If a local is live at
  BB exit, treat it conservatively.
- **Respect borrows**: `ImmBorrowLoc(x)` / `MutBorrowLoc(x)` require `x` to
  have a backing slot. A pre-pass marks such locals as *pinned*; pinned
  locals are never eliminated.
- **Respect calls**: a `Call` / `CallGeneric` may mutate memory reachable
  through any reference passed to it, so all borrow-tracked locals become
  opaque after a call. (Values on the stack are not affected — only
  reference-reachable memory.)
- **Respect types**: every transformation must preserve the stack type at
  every PC; the verifier re-runs after the pass.

## What we *can* eliminate without new opcodes

The core observation: a chain `[load x]; StLoc(y); ... use y` can be rewritten
to `... use x` (possibly as `CopyLoc(x)` instead of `MoveLoc(x)` depending on
live-in semantics), provided `y` has no other uses and is not pinned.

The chain shortens by 2 instructions (`load x; StLoc y` disappear), and the
user of `y` becomes a load of `x` — net save of 2 dispatches per rename.

### Transformations

| # | Pattern | Precondition | Rewrite |
|---|---|---|---|
| T1 | `MoveLoc(x); StLoc(y); ... MoveLoc(y)` | y not pinned; y has no other reads in the BB; y dead at BB exit | Delete `MoveLoc(x); StLoc(y)`. Replace `MoveLoc(y)` with `MoveLoc(x)`. |
| T2 | `CopyLoc(x); StLoc(y); ... MoveLoc(y)` | same as T1; x not invalidated between StLoc and use | Delete `CopyLoc(x); StLoc(y)`. Replace `MoveLoc(y)` with `CopyLoc(x)`. |
| T3 | `CopyLoc(x); StLoc(y); ... [CopyLoc(y)]+; MoveLoc(y)` | same + x still readable at each use site | Delete `CopyLoc(x); StLoc(y)`. Retarget each `CopyLoc(y)` and the final `MoveLoc(y)` to read from `x`. |
| T4 | `StLoc(x)` where x is never read in the BB and dead at exit | x not pinned | Only useful if preceded by an eliminable load: the whole preceding `load x; StLoc(x)` pair disappears and the stack top is consumed naturally by the next instruction. Otherwise we'd have to emit `Pop`, which is no win. |
| T5 | `CopyLoc(x)` as the final read of x (x dead at exit) | x not pinned; no later borrows | Rewrite to `MoveLoc(x)`. Same instruction count, but avoids a copy at runtime. |

Patterns we cannot handle without new opcodes:
- `StLoc(x); CopyLoc(x)` when `x` has later reads → would need `Dup`.
- `StLoc; StLoc` unpack → would need `StLocPair`.
- `MoveLoc(x); Ret` → would need `RetLoc`.

Those stay on the table for a future v2 that introduces super-instructions.

## Abstract state

A forward pass over the instructions of one basic block maintains:

```rust
enum StackSlot {
    /// The top-of-stack value at this abstract position was pushed by
    /// the given load instruction at `src_pc`.
    FromLoad {
        src_pc: usize,        // index of the CopyLoc / MoveLoc / LdXxx
        local: Option<u8>,    // Some(idx) if CopyLoc/MoveLoc, None for consts
        moved: bool,          // true if MoveLoc (local is dead past src_pc)
    },
    /// Produced by arithmetic, a call, a field-borrow, etc. Opaque.
    Computed,
}

struct BBState {
    // Abstract operand stack. Grows with pushes, shrinks with pops.
    stack: Vec<StackSlot>,

    // For each local, the PC of the most recent StLoc that is still a
    // candidate for elimination (not yet read, not yet invalidated).
    pending_store: HashMap<u8, PendingStore>,

    // Locals whose address escapes: either they are borrowed anywhere in
    // the function or their value is passed to a call that takes a
    // reference. Precomputed once per function.
    pinned: BitSet,
}

struct PendingStore {
    st_pc: usize,           // PC of the StLoc
    source: StackSlot,      // what was on stack when we stored
    uses: SmallVec<usize>,  // PCs of CopyLoc/MoveLoc of this local after st_pc
    sealed: bool,           // true once we've seen MoveLoc (no more reads allowed)
}
```

The pass walks the BB in one forward scan, updating this state and recording
**rewrite decisions** (a list of `(pc, Action)` where `Action` is `Delete`,
`ReplaceWith(new_opcode)`, etc.). After the scan, a second pass applies the
rewrites and emits the compacted bytecode.

## Per-opcode transition rules

For each instruction we read, we update `stack` / `pending_store` and decide
whether to mark it for deletion:

- **`LdConst`, `LdU8..256`, `LdTrue`, `LdFalse`**: push `Computed` (constants
  aren't worth tracking in v1).
- **`CopyLoc(x)` / `MoveLoc(x)`**: push `FromLoad { src_pc, local: Some(x), moved }`.
  If there is a `pending_store` for `x`, record a use at this PC. If `moved`,
  seal the `pending_store`.
- **`StLoc(x)`**:
  - If `x` is pinned, emit as-is; clear `pending_store[x]`.
  - Else, pop the stack; create a fresh `PendingStore { st_pc, source: popped, uses: [], sealed: false }` for `x`.
- **`ImmBorrowLoc(x)` / `MutBorrowLoc(x)`**: in a well-formed function these
  are only reachable if `x` is pinned; still, as a safety belt, commit any
  pending store for `x` and mark pinned.
- **`ReadRef` / `WriteRef`**: opaque; invalidate any pending store whose
  `source` is a `Computed` that came from a borrow (v1 just invalidates
  everything — conservative and cheap).
- **`Call*` / `CallGeneric*`**: pop N, push M; clear **all** pending stores
  (conservative: a callee given any reference can write through it).
- **Arithmetic, `Eq`, `Lt`, etc.**: pop operands, push `Computed`.
- **Branches (`BrTrue`/`BrFalse`/`Branch`/`Ret`)**: end of BB; commit all
  surviving pending stores (they need to persist across the branch).

After the scan completes, for each `PendingStore` that survived to BB end:
- If the local is dead at BB exit (from a cheap conservative live-out set
  described below), and has zero uses, delete the `StLoc` AND its source
  load as a pair (T4 fires).
- Otherwise, leave the `StLoc` in place.

For each `PendingStore` that saw exactly one `MoveLoc` use within the BB
before being sealed (or zero uses followed by the local being dead at exit):
- Apply T1/T2/T3: delete the source load + `StLoc`, retarget the uses.

## Liveness at BB exit — cheap approximation

True liveness needs inter-procedural data flow. For v1 we use a conservative
stand-in:

> A local `x` is considered **live at BB exit** if any successor BB contains
> a read of `x` that is not preceded (in that BB) by a write to `x`.

For the cheap version, we can simplify even further: compute per-BB **local
usage sets** (reads, writes) in one upfront linear pass over the function,
then define live-out as:

```
live_out(BB) = ⋃ over successors S of (reads(S) ∪ live_out(S)) \ writes_before_read(S)
```

A fixpoint iteration over the CFG converges in O(blocks × locals). For the
function sizes we see in practice (average ~14 BBs per function) this is
effectively free.

If the cost ever matters, a simpler over-approximation is to treat every
local as live at BB exit, which disables T4 (dead-store elimination) but
still enables T1/T2/T3 (the dominant savings) entirely, since those only
need "y has no later reads in this BB."

## Correctness-preserving invariants

At every PC in the rewritten BB, the following must hold:
1. **Stack depth** is unchanged from the original.
2. **Stack types** at every PC match the original (verifier re-runs).
3. **Local values at BB exit** match the original for every local that is
   live at exit or pinned.
4. **Observable side effects** (calls, writes through references) happen in
   the same order with the same operands.

Proof sketch for T1 (`MoveLoc(x); StLoc(y); ... MoveLoc(y)`):
- Original: pushes x's value onto the stack, stores into y, ..., loads y.
- Rewritten: deletes the `MoveLoc(x); StLoc(y)`, replaces `MoveLoc(y)` with
  `MoveLoc(x)`.
- Stack effect: original pushes once at the `MoveLoc(x)`, pops once at
  `StLoc(y)`, pushes once at `MoveLoc(y)` — net +1. Rewritten pushes once at
  the replaced `MoveLoc(x)` — net +1. ✓
- Types: x and y have the same type by construction (StLoc requires match). ✓
- y at exit: y was written only by the deleted `StLoc(y)` and not read after
  the deleted `MoveLoc(y)` (by precondition). y is dead. Rewritten leaves y
  unwritten, which equals its prior value; if y is dead at exit this is
  indistinguishable. ✓
- x at exit: was moved in the original, so its value at exit was either
  invalid or reassigned later. The rewritten code does the same move (just
  delayed to the later PC), so its final state matches. ✓

Analogous sketches for T2/T3/T5.

## Implementation

Lives in `external-crates/move/crates/move-vm-runtime/src/jit/optimization/`,
as a new module `bb_regalloc.rs`. Runs after the existing inliner pass
(`translate.rs::inline_direct_calls`) and before final bytecode emission.

```rust
pub(crate) fn optimize_basic_blocks(
    context: &FunctionContext,
    bytecode: Vec<Bytecode>,
) -> PartialVMResult<Vec<Bytecode>> {
    let cfg = build_cfg(&bytecode);
    let pinned = compute_pinned_locals(&bytecode);
    let live_out = compute_live_out(&cfg);
    let mut out = Vec::with_capacity(bytecode.len());
    for bb in cfg.basic_blocks() {
        let rewrites = analyze_bb(bb, &pinned, &live_out);
        emit_bb(&mut out, bb, &rewrites);
    }
    // Recompute branch targets because we deleted instructions.
    fixup_branches(&mut out, &original_offsets, &new_offsets)
}
```

Bytecode-offset fixups mirror what the inliner already does (see
`adjust_branch_targets`).

### File layout

- `bb_regalloc.rs` — the pass itself (one file, ~600-800 lines estimated)
- `bb_regalloc_tests.rs` — unit tests for each transformation in isolation
- Integration tests in `unit_tests/bb_regalloc_tests.rs`:
  - bytecode comparison (before/after) for each transformation pattern
  - execution correctness against a spectrum of Move programs
  - end-to-end test: run the inliner test suite with this pass enabled

## Expected impact

From the static analysis:

| Transformation | Static pairs | Instructions removed per firing |
|---|---:|---:|
| T1 pure rename | 7,356 | 2 |
| T2/T3 copy rename | 1,038 | 2 per use |
| T4 dead store elim | ~(unknown, need measurement) | 2 |
| T5 CopyLoc→MoveLoc | — | 0 (runtime-only win) |

Conservative projection: **~15K static instruction pairs eliminated across
our 542K sample = ~2.8% of static code size**. Weighted by dynamic execution
this likely lands at **3-6% of runtime instructions**, matching empirical
experience with peephole optimizers on similar VMs.

A future v2 adding `Dup`/`StLocPair`/`RetLoc` super-instructions on top of
this framework would roughly double the win.

## Rollout

1. **Land the pass behind a feature flag** (`bb_regalloc`) — off by default.
2. **Run the full simtest suite** with the flag on; verify no behavioral
   difference.
3. **Measure on the replay profile** — re-run the 10k-tx profile with the
   flag on and compare the dynamic opcode distribution.
4. **Bench `language-benchmarks`** (on and off). Expect 2-5% wall-clock
   improvement on the compute-bound cases.
5. **Flip the default** once numbers are clean.

## Open questions

1. **Should T4 (dead-store elim) be mandatory or opt-in?** It touches more
   code than T1-T3 and is sensitive to the live-out approximation. Starting
   with T1/T2/T3 only is tempting.
2. **What about consecutive `StLoc; StLoc` from struct unpacks?** This
   genuinely wants a super-instruction. Defer to v2.
3. **Runtime vs JIT-time cost**: we expect single-digit ms per package at
   JIT time. If this turns out to matter, cache the optimization result.

## Non-goals

- Introducing new opcodes (`Dup`, `StLocPair`, `CallN`, etc.) — deferred to v2.
- Inter-procedural analysis — stays in one function, one block at a time.
- Register allocation in the "assign each value to a physical register"
  sense — we keep the stack machine; we just stop emitting round-trips
  through locals.
- Beating a proper SSA optimizer — this is intentionally the simplest pass
  that captures most of the low-hanging fruit.
