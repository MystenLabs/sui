// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Rewrite forward `goto`s into labeled-block `break`s.
//
// After structuring, a forward control-flow edge the structurer couldn't lower to a
// structured break/continue survives as `Unstructured(Goto(N))`, with `Block(N, body)`
// (the goto's landing point) appearing LATER in the same `Seq`:
//
//   Seq[ ...prefix containing Unstructured(Goto(N))...,
//        Block(N, body),
//        ...suffix... ]
//
// Move's labeled blocks express exactly this: wrap the prefix in a `'label_N: { ... }`
// block, turn each `goto 'label_N` into `break 'label_N`, and move the landing body out to
// follow the block:
//
//   Seq[ Block(N, prefix_with_breaks), body..., suffix... ]
//
// Soundness: only fires when EVERY `Goto(N)` lives before the `Block(N)` (a forward jump).
// A backward `Goto(N)` (target earlier than the goto) is a loop the structurer didn't
// recover; labeled blocks can't express it, so we leave it as a goto. We also require the
// block body itself to contain no `Goto(N)` (that would be a self-loop, again backward).
//
// The `Goto -> Break(Some(N))` rewrite produces a first-class `Break`; the pretty-printer
// renders `Break(Some(N))` as `break 'label_N` when `N` is a block id (vs `break 'loop_N`
// for loops). `Block(N)` continues to render as `'label_N: { ... }` because it's still
// referenced (now by a break rather than a goto).

use crate::{
    ast::Exp,
    refinement::{
        Refine,
        utils::{contains_goto, rewrite_gotos_as_breaks},
    },
};

pub fn refine(exp: &mut Exp) -> bool {
    let mut pass = GotoToBreak { changed: false };
    pass.refine(exp);
    pass.changed
}

struct GotoToBreak {
    changed: bool,
}

impl Refine for GotoToBreak {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        if let Exp::Seq(items) = exp
            && try_wrap(items)
        {
            self.changed = true;
            return true;
        }
        false
    }
}

fn try_wrap(items: &mut Vec<Exp>) -> bool {
    for j in 0..items.len() {
        let Exp::Block(n, body) = &items[j] else {
            continue;
        };
        let n = *n;
        // The block body must not jump to its own label (that would be a backward/self
        // loop, not expressible as a forward break).
        if contains_goto(body, n) {
            continue;
        }
        // Earliest prefix item that jumps to `n`.
        let Some(k) = (0..j).find(|&i| contains_goto(&items[i], n)) else {
            continue;
        };
        // Forward-only: nothing after the block may jump to `n`.
        if (j + 1..items.len()).any(|i| contains_goto(&items[i], n)) {
            continue;
        }

        // Pull out items[k..=j]; the last is the Block, the rest is the prefix to wrap.
        let mut removed: Vec<Exp> = items.splice(k..=j, std::iter::empty()).collect();
        let block = removed.pop().unwrap();
        let body_n = match block {
            Exp::Block(_, b) => *b,
            _ => unreachable!(),
        };
        // Rewrite gotos-to-`n` in the prefix into breaks-to-`n`.
        for item in &mut removed {
            rewrite_gotos_as_breaks(item, n);
        }
        let new_block = Exp::Block(n, Box::new(Exp::Seq(removed)));

        // Splice back: the wrapped block, then the landing body's items, at position k.
        let mut insert = vec![new_block];
        match body_n {
            Exp::Seq(v) => insert.extend(v),
            other => insert.push(other),
        }
        items.splice(k..k, insert);
        return true;
    }
    false
}
