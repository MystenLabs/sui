# NMG §V-B implementation plan for `-1-VB`

## Goal

Replace the dispatch-table machinery (`emit_dispatch_arms` + `SelectorMatch`) with a
reaching-condition cascade for multi-owned-succ loops. The output should be a single-exit
loop followed by a `CondIf` chain that uses the body's recovered boolean formulas, not a
synthetic `__dispatch_<N>` selector.

Concretely, pyth's `update_all` should go from

```move
loop {
    if (l12 < l14) { ... ; l12 += 1; continue };
    if (l13) { l12 = 0; __dispatch_15 = 0; break };
    let l18 = vaa::parse_and_verify(...);
    ... ; __dispatch_15 = 1; break
};
match (__dispatch_15) {
    0 => { ...tag 0 body... },
    1 => { ...tag 1 body... }
}
```

to something like

```move
loop {
    if (l12 < l14) { ... ; l12 += 1; continue };
    break
};
if (l13) {
    ...tag 0 body...
} else {
    let l18 = vaa::parse_and_verify(...);
    ...tag 1 body...
}
```

(The `l13` vs `__cN` recovery is a separate concern; what §V-B itself delivers is the
disappearance of `__dispatch_15` and the `match`.)

## Pieces

1. **NCD on the dom tree.** Implemented: `DominatorTree::nearest_common_dominator` walks
   the idom chains up to their lowest common ancestor.

2. **Body acyclic projection.** Build a `BTreeMap<NodeIndex, D::Input>` over `loop_nodes`,
   with edges back to `loop_head` dropped (so the region is acyclic) and `owned_succs`
   added as `Input::Code(_, _, None)` sinks (so `reaching_conditions` produces formulas
   for them).

3. **Reaching conditions.** Call `acyclic::reaching_conditions(&projection, loop_head)`.
   Pull `c(loop_head, succ)` for each owned succ. Atoms in those formulas are condition
   blocks inside the body (via `cond_atom`); `Box<Option<Structured>>` cascade arms reuse
   the same atoms, so the printer will share `let __cN = ...` bindings naturally.

4. **Cascade emission.** Replace `emit_dispatch_arms` with `emit_vb_cascade`:
   ```rust
   fn emit_vb_cascade(
       body: D::Structured,
       owned_succs: &[NodeIndex],
       formulas: &BTreeMap<NodeIndex, Formula>,
       structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
       loop_head: NodeIndex,
   ) -> D::Structured {
       // Pick primary = max-index owned_succ (it becomes the trailing else).
       // For each non-primary succ: prepend `if (formula) { succ.body } else { rest }`.
       // Wrap the loop and append the cascade.
   }
   ```

5. **Body restructuring.** The body, structured by reaching, currently emits
   `Jump(ReachingExit, owned_succ)` for each abnormal exit. `insert_breaks(loop_successor=
   None)` leaves these as raw `Jump`s. With §V-B, we need each one to become a plain
   `Break(loop_head)` -- the cascade decides afterward where to go based on the formula.
   So either:
   - Extend `insert_breaks` with a "treat all owned_succs as breaks" mode, OR
   - Post-process the structured body after reaching: walk it, replace each
     `Jump(_, owned_succ)` with `Break(loop_head)`.

6. **Multi-exit reaching gate lift.** With the cascade, the existing reaching gate on
   `multi_successor_mode` can be lifted (see `MULTI_EXIT_LOOPS.md`). The body reaching
   now sees a clean single-exit region.

## Fallback

If `reaching_conditions` fails (region cycle, `Variants`) OR the resulting formulas have
too many atoms to be readable (heuristic: `formula.cond_atoms().len() > N`), fall back to
the existing `emit_dispatch_arms` path.

## Open questions

- Is the heuristic for "formula too complex" worth having? NMG observed that DREAM produced
  9600 boolean operators vs 1256 in source; we should cap at something modest. SAILR's
  data suggests ~10 atoms is the readability cliff.
- Where does `recover_flag` (PR-2 today) sit relative to §V-B? Likely §V-B subsumes it for
  this specific pattern, but `recover_flag` may still be useful for non-loop flag patterns.
  Evaluate after §V-B lands.
- Should we keep `compress_dispatch_cascade` (PR-4)? It compresses the SelectorMatch that
  §V-B eliminates. Likely no longer needed for §V-B-handled loops; evaluate after.
