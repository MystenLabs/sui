// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Basic-block-local stack-aware optimizer.
//!
//! See `DESIGN-bb-regalloc.md` in this directory for the full design.
//!
//! # v1 transformations
//!
//! Given a pure local-load followed by an immediate `StLoc` and a single later
//! read in the same basic block, the load/store pair is deleted and the read
//! is retargeted to the original source local.
//!
//! Patterns handled:
//!
//! - **T1** `MoveLoc(x); StLoc(y); ... MoveLoc(y)` where `y` has exactly one
//!   read in the BB after `StLoc(y)` and no further write. Rewrite: delete the
//!   first two instructions and replace the use with `MoveLoc(x)`.
//!
//! - **T2** `CopyLoc(x); StLoc(y); ... MoveLoc(y)` under the same conditions.
//!   Rewrite: delete the pair and replace the use with `CopyLoc(x)`.
//!
//! - **Variants** where the single read is `CopyLoc(y)` instead of `MoveLoc(y)`
//!   are handled identically (the kind of the retargeted load is determined
//!   by the *source* load, not by the use).
//!
//! Deferred to v2 (require new opcodes or cross-block analysis):
//!
//! - `StLoc(x); CopyLoc(x)` store-then-reload (needs `Dup`).
//! - `StLoc; StLoc` unpack fusion (needs `StLocPair`).
//! - `MoveLoc; Ret` tail return fusion (needs `RetLoc`).
//! - Dead-store elimination that isn't covered by the pair rule.
//! - Multi-use copy propagation (`CopyLoc(x); StLoc(y); CopyLoc(y)+; MoveLoc(y)`).
//!
//! # Safety
//!
//! The pass is sound under these invariants:
//!
//! 1. `y` is never borrowed anywhere in the function (checked by
//!    [`compute_pinned`]). Pinned locals retain their backing slot.
//! 2. `x` is also never borrowed (same check).
//! 3. Between the deleted source load and the retargeted use, no instruction
//!    writes to `x` (checked per-pattern).
//! 4. `y` has exactly one read in the BB between `StLoc(y)` and the next write
//!    of `y` (or end of block); that read is the one being retargeted.
//! 5. The pass is BB-local. It never crosses block boundaries and so cannot
//!    invalidate branch targets or inter-block live-in/out.
//!
//! Calls, `WriteRef`, and other opaque operations cannot affect non-pinned
//! locals (the only way to mutate a local is via `StLoc` or through a borrow
//! of that local, neither of which can target a non-pinned local from opaque
//! code).

use crate::jit::optimization::ast::{Bytecode, Code, Function, Module, Package};
use move_binary_format::file_format::LocalIndex;

/// Run the optimizer over every function in the package.
#[cfg_attr(not(feature = "bb_regalloc"), allow(dead_code))]
pub fn optimize_package(mut pkg: Package) -> Package {
    for module in pkg.modules.values_mut() {
        optimize_module(module);
    }
    pkg
}

#[cfg_attr(not(feature = "bb_regalloc"), allow(dead_code))]
fn optimize_module(module: &mut Module) {
    for func in module.functions.values_mut() {
        optimize_function(func);
    }
}

#[cfg_attr(not(feature = "bb_regalloc"), allow(dead_code))]
fn optimize_function(func: &mut Function) {
    let Some(code) = func.code.as_mut() else {
        return;
    };
    let pinned = compute_pinned(code);
    for block in code.code.values_mut() {
        let optimized = optimize_block(block, &pinned);
        *block = optimized;
    }
}

/// Set of locals that are ever borrowed anywhere in the function.
///
/// A borrowed local retains its backing slot (the reference points to it),
/// so we cannot eliminate any store to it. Computed once per function by a
/// single linear scan of all basic blocks.
///
/// Represented as a `Vec<bool>` indexed by `LocalIndex` (at most 256 slots).
fn compute_pinned(code: &Code) -> Vec<bool> {
    let mut max_local_plus_one: usize = 0;
    for block in code.code.values() {
        for bc in block {
            if let Some(idx) = local_index(bc) {
                let next = (idx as usize).saturating_add(1);
                if next > max_local_plus_one {
                    max_local_plus_one = next;
                }
            }
        }
    }
    let mut pinned = vec![false; max_local_plus_one];
    for block in code.code.values() {
        for bc in block {
            if let Bytecode::ImmBorrowLoc(x) | Bytecode::MutBorrowLoc(x) = bc {
                let idx = *x as usize;
                if let Some(slot) = pinned.get_mut(idx) {
                    *slot = true;
                }
            }
        }
    }
    pinned
}

/// Local slot referenced by a local-touching bytecode, or `None`.
fn local_index(bc: &Bytecode) -> Option<LocalIndex> {
    match bc {
        Bytecode::CopyLoc(x)
        | Bytecode::MoveLoc(x)
        | Bytecode::StLoc(x)
        | Bytecode::ImmBorrowLoc(x)
        | Bytecode::MutBorrowLoc(x) => Some(*x),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum LoadKind {
    Copy(LocalIndex),
    Move(LocalIndex),
}

fn as_load(bc: &Bytecode) -> Option<LoadKind> {
    match bc {
        Bytecode::CopyLoc(x) => Some(LoadKind::Copy(*x)),
        Bytecode::MoveLoc(x) => Some(LoadKind::Move(*x)),
        _ => None,
    }
}

fn is_pinned(pinned: &[bool], idx: LocalIndex) -> bool {
    pinned.get(idx as usize).copied().unwrap_or(false)
}

enum Action {
    Keep,
    Delete,
    Replace(Bytecode),
}

/// Apply v1 rename elimination to a single basic block and return the rewritten
/// sequence. If nothing is rewritten, the returned vector is a cheap clone of
/// the input.
fn optimize_block(block: &[Bytecode], pinned: &[bool]) -> Vec<Bytecode> {
    if block.len() < 3 {
        // Smallest rewriteable pattern is `load; StLoc; use` → 3 instructions.
        return block.to_vec();
    }
    let mut plan: Vec<Action> = (0..block.len()).map(|_| Action::Keep).collect();
    let mut pc = 0usize;
    while pc < block.len() {
        let next_pc = pc.saturating_add(1);
        // We look for a `StLoc(y)` preceded by a load; skip quickly otherwise.
        let y = match block.get(pc) {
            Some(Bytecode::StLoc(y)) => *y,
            _ => {
                pc = next_pc;
                continue;
            }
        };
        if pc == 0 || is_pinned(pinned, y) {
            pc = next_pc;
            continue;
        }
        let load_pc = pc.saturating_sub(1);
        // If the preceding instruction is already scheduled for a rewrite,
        // bail: composing rewrites is deferred to v2.
        if !matches!(plan.get(load_pc), Some(Action::Keep)) {
            pc = next_pc;
            continue;
        }
        let kind = match block.get(load_pc).and_then(as_load) {
            Some(k) => k,
            None => {
                pc = next_pc;
                continue;
            }
        };
        let x = match kind {
            LoadKind::Copy(x) | LoadKind::Move(x) => x,
        };
        if is_pinned(pinned, x) || x == y {
            // x == y would be a self-copy/self-move then self-store — unusual,
            // and the semantics of MoveLoc(x); StLoc(x) differ from the no-op
            // case (MoveLoc invalidates x). Skip to keep v1 tight.
            pc = next_pc;
            continue;
        }

        // Scan forward looking for the single read of y, while tracking whether
        // x is still in its original state.
        let (use_pc, safe) = find_sole_y_use(block, pc, x, y);
        if !safe {
            pc = next_pc;
            continue;
        }
        let Some(use_pc) = use_pc else {
            pc = next_pc;
            continue;
        };

        // Plan the rewrite.
        if let Some(slot) = plan.get_mut(load_pc) {
            *slot = Action::Delete;
        }
        if let Some(slot) = plan.get_mut(pc) {
            *slot = Action::Delete;
        }
        if let Some(slot) = plan.get_mut(use_pc) {
            *slot = Action::Replace(match kind {
                LoadKind::Copy(x) => Bytecode::CopyLoc(x),
                LoadKind::Move(x) => Bytecode::MoveLoc(x),
            });
        }
        pc = use_pc.saturating_add(1);
    }

    emit(block, plan)
}

/// Find the single read of `y` after `st_pc` in the current BB, subject to:
/// - No other read of `y` may appear before the next write of `y` (or end).
/// - Between `st_pc` (exclusive) and the found use (exclusive), neither `StLoc(x)`
///   nor `MoveLoc(x)` may appear (those would change the value of `x`).
///
/// Returns `(Some(use_pc), true)` on success. Returns `(None, true)` if `y` has
/// zero reads in the BB (rewrite not eligible under v1). Returns `(_, false)` if
/// the pattern is unsafe (multiple reads, `x` mutated in between, etc.).
fn find_sole_y_use(
    block: &[Bytecode],
    st_pc: usize,
    x: LocalIndex,
    y: LocalIndex,
) -> (Option<usize>, bool) {
    let mut found: Option<usize> = None;
    for (i, bc) in block.iter().enumerate().skip(st_pc.saturating_add(1)) {
        // A later write to `y` invalidates the pending StLoc — stop the scan
        // here. We neither accept nor reject; we report "no eligible use found".
        if matches!(bc, Bytecode::StLoc(yy) if *yy == y) {
            break;
        }
        // A later read of `y` — is it our single use?
        if let Bytecode::CopyLoc(yy) | Bytecode::MoveLoc(yy) = bc
            && *yy == y
        {
            if found.is_some() {
                return (None, false); // multiple reads
            }
            found = Some(i);
            continue;
        }
        // x must not be mutated/consumed between st_pc and the use.
        // CopyLoc(x) is fine — x remains readable.
        if matches!(bc, Bytecode::StLoc(xx) if *xx == x)
            || matches!(bc, Bytecode::MoveLoc(xx) if *xx == x)
        {
            return (None, false);
        }
    }
    (found, true)
}

fn emit(block: &[Bytecode], plan: Vec<Action>) -> Vec<Bytecode> {
    let mut out = Vec::with_capacity(block.len());
    for (bc, action) in block.iter().zip(plan.into_iter()) {
        match action {
            Action::Keep => out.push(bc.clone()),
            Action::Delete => {}
            Action::Replace(new) => out.push(new),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn pinned_none() -> Vec<bool> {
        // Enough slots for the tests to avoid out-of-bounds checks masking
        // missing pin info.
        vec![false; 8]
    }

    fn debug(seq: &[Bytecode]) -> String {
        seq.iter()
            .map(|bc| format!("{:?}", bc))
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn assert_block_eq(got: &[Bytecode], expected: &[Bytecode]) {
        assert_eq!(
            debug(got),
            debug(expected),
            "blocks differ:\n  got:  {}\n  want: {}",
            debug(got),
            debug(expected),
        );
    }

    #[test]
    fn t1_pure_rename_moveloc_then_moveloc() {
        // MoveLoc(0); StLoc(1); Add; MoveLoc(1); Ret
        //   -> Add; MoveLoc(0); Ret   (StLoc and source MoveLoc deleted; use retargeted)
        let block = vec![
            Bytecode::MoveLoc(0),
            Bytecode::StLoc(1),
            Bytecode::Add,
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &[Bytecode::Add, Bytecode::MoveLoc(0), Bytecode::Ret]);
    }

    #[test]
    fn t2_copy_rename_copyloc_then_moveloc() {
        // CopyLoc(0); StLoc(1); ... ; MoveLoc(1)
        //   -> ... ; CopyLoc(0)
        let block = vec![
            Bytecode::CopyLoc(0),
            Bytecode::StLoc(1),
            Bytecode::Add,
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &[Bytecode::Add, Bytecode::CopyLoc(0), Bytecode::Ret]);
    }

    #[test]
    fn t1_use_is_copyloc_still_eligible() {
        // MoveLoc(0); StLoc(1); ReadRef; CopyLoc(1); Ret
        // Only one read of y=1 (CopyLoc) and no subsequent write to y, so rewrite.
        // Source was MoveLoc, so retargeted load is MoveLoc(0).
        let block = vec![
            Bytecode::MoveLoc(0),
            Bytecode::StLoc(1),
            Bytecode::ReadRef,
            Bytecode::CopyLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(
            &out,
            &[Bytecode::ReadRef, Bytecode::MoveLoc(0), Bytecode::Ret],
        );
    }

    #[test]
    fn bail_when_y_has_multiple_reads() {
        // CopyLoc(0); StLoc(1); CopyLoc(1); CopyLoc(1); Ret
        // y=1 has two reads -> v1 bails, block unchanged.
        let block = vec![
            Bytecode::CopyLoc(0),
            Bytecode::StLoc(1),
            Bytecode::CopyLoc(1),
            Bytecode::CopyLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &block);
    }

    #[test]
    fn bail_when_y_is_pinned() {
        // Same as t1 but y=1 is pinned.
        let block = vec![
            Bytecode::MoveLoc(0),
            Bytecode::StLoc(1),
            Bytecode::Add,
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let mut pinned = pinned_none();
        pinned[1] = true;
        let out = optimize_block(&block, &pinned);
        assert_block_eq(&out, &block);
    }

    #[test]
    fn bail_when_x_is_pinned() {
        // Same as t2 but x=0 is pinned.
        let block = vec![
            Bytecode::CopyLoc(0),
            Bytecode::StLoc(1),
            Bytecode::Add,
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let mut pinned = pinned_none();
        pinned[0] = true;
        let out = optimize_block(&block, &pinned);
        assert_block_eq(&out, &block);
    }

    #[test]
    fn bail_when_x_is_overwritten_between_load_and_use() {
        // MoveLoc(0); StLoc(1); <something that STORES to 0>; MoveLoc(1)
        // Retargeting MoveLoc(1) → MoveLoc(0) would read the NEW x. Must bail.
        let block = vec![
            Bytecode::MoveLoc(0),
            Bytecode::StLoc(1),
            Bytecode::LdU64(42),
            Bytecode::StLoc(0), // overwrites x=0
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &block);
    }

    #[test]
    fn bail_when_x_is_moved_between_load_and_use() {
        // CopyLoc(0); StLoc(1); MoveLoc(0); Pop; MoveLoc(1); Ret
        // x=0 is moved (invalidated) before use — bail.
        let block = vec![
            Bytecode::CopyLoc(0),
            Bytecode::StLoc(1),
            Bytecode::MoveLoc(0),
            Bytecode::Pop,
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &block);
    }

    #[test]
    fn intermediate_copy_of_x_is_fine() {
        // CopyLoc(0); StLoc(1); CopyLoc(0); Pop; MoveLoc(1); Ret
        // Intermediate CopyLoc(0) does not invalidate x — still readable.
        let block = vec![
            Bytecode::CopyLoc(0),
            Bytecode::StLoc(1),
            Bytecode::CopyLoc(0),
            Bytecode::Pop,
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(
            &out,
            &[
                Bytecode::CopyLoc(0),
                Bytecode::Pop,
                Bytecode::CopyLoc(0),
                Bytecode::Ret,
            ],
        );
    }

    #[test]
    fn bail_when_y_is_zero_use_in_block() {
        // MoveLoc(0); StLoc(1); Ret
        // y=1 has zero reads in this BB. Could be live at BB exit; v1 bails
        // (dead-store elimination is out of scope).
        let block = vec![Bytecode::MoveLoc(0), Bytecode::StLoc(1), Bytecode::Ret];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &block);
    }

    #[test]
    fn bail_when_y_is_written_again_before_use() {
        // MoveLoc(0); StLoc(1); LdU64(5); StLoc(1); MoveLoc(1); Ret
        // y=1 is overwritten before its one use; the first StLoc is a dead
        // store but v1 doesn't eliminate those. Block unchanged.
        let block = vec![
            Bytecode::MoveLoc(0),
            Bytecode::StLoc(1),
            Bytecode::LdU64(5),
            Bytecode::StLoc(1),
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &block);
    }

    #[test]
    fn two_nonoverlapping_rewrites_in_one_block() {
        // Two independent rename chains in one block; both should fire.
        // MoveLoc(0); StLoc(2); MoveLoc(2); Pop; MoveLoc(1); StLoc(3); MoveLoc(3); Ret
        let block = vec![
            Bytecode::MoveLoc(0),
            Bytecode::StLoc(2),
            Bytecode::MoveLoc(2),
            Bytecode::Pop,
            Bytecode::MoveLoc(1),
            Bytecode::StLoc(3),
            Bytecode::MoveLoc(3),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(
            &out,
            &[
                Bytecode::MoveLoc(0),
                Bytecode::Pop,
                Bytecode::MoveLoc(1),
                Bytecode::Ret,
            ],
        );
    }

    #[test]
    fn compute_pinned_detects_immborrow() {
        let mut code = Code {
            jump_tables: vec![],
            code: std::collections::BTreeMap::new(),
        };
        code.code
            .insert(0, vec![Bytecode::ImmBorrowLoc(3), Bytecode::Ret]);
        let pinned = compute_pinned(&code);
        assert_eq!(pinned.len(), 4);
        assert!(!pinned[0]);
        assert!(!pinned[1]);
        assert!(!pinned[2]);
        assert!(pinned[3]);
    }

    #[test]
    fn compute_pinned_detects_mutborrow() {
        let mut code = Code {
            jump_tables: vec![],
            code: std::collections::BTreeMap::new(),
        };
        code.code
            .insert(0, vec![Bytecode::MutBorrowLoc(2), Bytecode::Ret]);
        let pinned = compute_pinned(&code);
        assert!(pinned[2]);
    }

    #[test]
    fn self_load_store_is_skipped() {
        // MoveLoc(1); StLoc(1); ... MoveLoc(1) — x == y. MoveLoc(x); StLoc(x)
        // is a specific pattern we don't touch in v1 (semantics are subtle
        // because MoveLoc invalidates x before the store).
        let block = vec![
            Bytecode::MoveLoc(1),
            Bytecode::StLoc(1),
            Bytecode::MoveLoc(1),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &block);
    }

    #[test]
    fn block_under_three_instructions_untouched() {
        let block = vec![Bytecode::MoveLoc(0), Bytecode::Ret];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &block);
    }

    #[test]
    fn short_circuit_skips_plan_conflicts() {
        // Interleaved patterns where two candidate rewrites compete for the
        // same load_pc should not double-apply. v1 takes the first.
        // MoveLoc(0); StLoc(1); StLoc(2); MoveLoc(2); Ret
        // At pc=2 (StLoc(2)), load_pc=1 is a StLoc(1), not a load. Skip.
        // No pattern rewrites; block unchanged.
        let block = vec![
            Bytecode::MoveLoc(0),
            Bytecode::StLoc(1),
            Bytecode::StLoc(2),
            Bytecode::MoveLoc(2),
            Bytecode::Ret,
        ];
        let out = optimize_block(&block, &pinned_none());
        assert_block_eq(&out, &block);
    }
}
