# Multi-exit loop refinement (follow-up to PR #1)

## Status

`loops::structure_loop` currently gates reaching off for `multi_successor_mode` loops
on the main path. The dom-tree structurer handles them, then `emit_dispatch_arms`
synthesizes the `__dispatch_<N>` selector + `SelectorMatch` tail. This gate has
existed since before the PR-1 refactor; it's a workaround, not a regression.

To lift the gate we need to make the body single-exit *before* reaching runs, so
the walker sees a clean SESE region with one back-edge to `loop_head` and one
synthesized post-loop continuation.

## Why the naive synthesis didn't work

The first attempt (now reverted) installed one synthetic `Input::Reduced(synth_S, [])`
per owned succ `S`, with structured form `Assign(sel, k); Break(loop_head)`, and
rewrote body edges to `S` to point at `synth_S`. This is structurally still
multi-exit -- each abnormal exit has its own sink in the PostDom graph -- so
`ipostdom(loop_head)` still resolved to the synthetic exit (= `None` in the walker),
which means the duplicate-arm fold's `rest = structure_reachable_subregion(far_join, stop)`
still gets `stop = None` and can walk past the back-edge into the dispatch arm.

The duplication we observe in pyth's `update_all` comes from this: reaching processes
the staleness fold's `rest` from `far_join` and, with `stop = None`, descends into the
dispatch decision (originally placed in the else-arm of `loop_head`'s Condition),
emitting its structured form there too.

## What we should do instead (NMG §V-B)

NMG §V-B describes the proper multi-exit refinement:

1. Pick a primary successor (kept as the loop's normal exit). Today's
   `try_structure_loop_without_dispatch` already picks `primary = min(owned_succs)`.
2. Let `E_out = {(n, u) | n in loop_nodes, u in owned_succs, u != primary}` --
   the abnormal-exit edges.
3. Compute `n_ncd` = nearest common dominator of source nodes in `E_out`,
   computed over `graph.dom_tree` restricted to `loop_nodes`. The paper notes
   `n_ncd` is always a loop node (the loop header dominates all loop nodes).
4. Compute reaching conditions `c(n_ncd, u_i)` for each abnormal target `u_i`,
   using the existing `acyclic::reaching_conditions` machinery restricted to the
   sub-region from `n_ncd` to the abnormal exits.
5. Splice a synthetic cascade node into the `Input` map at `n_ncd`. Its body is
   `if (c_1) goto u_1; else if (c_2) goto u_2; ...; else fall_through_to_primary`,
   and the original `E_out` edges are removed (their sources now flow to the
   cascade instead). The cascade's structured form is built directly from the
   reaching-condition formulas.

After this transformation the body has exactly two exits: the back-edge to
`loop_head` and one path to `primary`. Reaching then structures it as a
single-exit region and `emit_single_exit_loop` wraps it normally; the cascade
inside the body has already done the dispatch by the time we reach the loop end.

## Implementation notes

- NCD is straightforward over `graph.dom_tree`: walk both source paths up the
  dom tree until they converge.
- `reaching_conditions` already returns a `BTreeMap<NodeIndex, Formula>` over an
  acyclic region; we'd call it on `loop_nodes` restricted to the sub-region
  reachable from `n_ncd` (excluding the back-edge).
- The synthetic cascade is a new `Input::Code` or a sequence of synthetic Conditions
  representing the `if (c_i) goto u_i` chain. We may want a dedicated
  `Input::Cascade` variant if we want it printed distinctly, or we can just lower
  it to a `CondIf` chain in `structured_blocks` and reference via `Input::Reduced`.
- The §V-B transform is the right place to also reconsider the
  `try_structure_loop_without_dispatch` speculation: once we have the cascade,
  the speculation may always succeed (or never need to run).

## Why this isn't in PR #1

PR #1 is the reaching-condition acyclic structurer plus the region-selection
plumbing it requires. §V-B is a separate transformation on cyclic regions that
depends on the acyclic machinery but operates above it. Shipping §V-B in PR #1
would mean writing the NCD pass, the reaching-condition restriction, the
synthetic-cascade splicer, and the corresponding refinements -- a meaningful
chunk of work that deserves its own PR with its own test corpus expansion.
