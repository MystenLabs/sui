// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Cyclic-region structuring.
//!
//! Per loop head identified by `Graph::loop_heads`, run `structure_loop`. It wraps the
//! loop body via reaching (preferred) or dom-tree (fallback), placing post-loop bodies and
//! synthesizing a dispatch table on multi-owned-succ shapes. On completion, an
//! `Input::Reduced(loop_head, succs)` marker replaces the body in `input` so outer scopes
//! treat the loop as an opaque NMG §IV-C abstract node.

use crate::structuring::{
    StructureContext, acyclic,
    ast::{self as D, GotoSource},
    graph::Graph,
    predicates::{self, Formula},
};

use petgraph::{Direction, graph::NodeIndex};

use std::collections::{BTreeMap, HashMap, HashSet};

#[allow(clippy::expect_fun_call)]
pub(super) fn structure_loop(
    ctx: StructureContext<'_>,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    loop_head: NodeIndex,
    input: &mut BTreeMap<D::Label, D::Input>,
) {
    let config = ctx.config;
    let terms = ctx.terms;
    if config.debug_print.structuring {
        println!("structuring loop at node {loop_head:#?}");
    }
    let (loop_nodes, succ_nodes) = graph.find_loop_nodes(loop_head);

    // Partition succs into "owned by this loop's scope" (dominated by `loop_head`) and the
    // rest. Only owned succs become part of this loop's structure - unowned ones are
    // latch-bound to an enclosing scope (outer-loop break/continue target, or a join in an
    // ancestor's region) and their Jumps stay raw for the outer pass to handle.
    let owned_succs: Vec<NodeIndex> = {
        let mut v: Vec<NodeIndex> = graph
            .dom_tree
            .get(loop_head)
            .all_children()
            .filter(|n| succ_nodes.contains(n))
            .collect();
        v.sort_by_key(|n| n.index());
        v
    };
    if config.debug_print.structuring {
        println!("  loop nodes: {loop_nodes:#?}");
        println!("  successor nodes: {succ_nodes:#?}");
        println!("  owned succs: {owned_succs:?}");
    }

    // Single-exit: `insert_breaks` rewrites Jumps to `min(succ_nodes)` as `Break`.
    // Multi-owned-succ: synthesize a selector local + match. First try a speculative
    // single-exit layout (`try_structure_loop_without_dispatch`); if the body has a residual
    // Jump to a non-primary owned succ after `insert_breaks`, fall back to dispatch.
    let multi_successor_mode = owned_succs.len() > 1;
    if multi_successor_mode {
        let mut spec_graph = graph.clone();
        let mut spec_blocks = structured_blocks.clone();
        let mut spec_input = input.clone();
        let primary = owned_succs.iter().copied().min().unwrap();
        let mut spec_absorbed: HashSet<NodeIndex> = HashSet::new();

        if try_structure_loop_without_dispatch(
            ctx,
            &mut spec_graph,
            &mut spec_blocks,
            &mut spec_input,
            loop_head,
            &loop_nodes,
            &succ_nodes,
            &owned_succs,
            primary,
            &mut spec_absorbed,
        ) {
            *graph = spec_graph;
            *structured_blocks = spec_blocks;
            *input = spec_input;
            install_reduced_marker(input, loop_head, &loop_nodes, &succ_nodes, &spec_absorbed);
            return;
        }
    }

    let loop_successor: Option<NodeIndex> = if multi_successor_mode {
        // Suppress single-target break-rewriting: in dispatch mode EVERY owned-succ Jump gets
        // rewritten to `Assign(sel, k); Break(loop_head)` by the dispatch pass below, not by
        // `insert_breaks`.
        None
    } else {
        succ_nodes.iter().copied().min()
    };

    acyclic::structure_acyclic_node(
        ctx,
        graph,
        structured_blocks,
        loop_head,
        input,
        /*loop_successor*/ loop_successor,
    );

    // Hand reaching the live `input` restricted to `loop_nodes`. `members = loop_nodes \
    // {loop_head}` makes back-edges to `loop_head` fire `!in_region`, emitting exit-jumps
    // that `insert_breaks` rewrites to `Continue`. Inner sub-loops are `Input::Reduced`
    // markers (post-order DFS) and walk via `process_reduced`. Dispatch mode is gated --
    // see notes in V_B_PLAN.md.
    let reaching_body: Option<D::Structured> = if !multi_successor_mode {
        let region_input: BTreeMap<D::Label, D::Input> = input
            .iter()
            .filter(|(k, _)| loop_nodes.contains(k))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        let members: HashSet<NodeIndex> = loop_nodes
            .iter()
            .copied()
            .filter(|n| *n != loop_head)
            .collect();
        acyclic::structure_acyclic(
            config,
            terms,
            structured_blocks,
            &region_input,
            loop_head,
            &members,
        )
    } else {
        None
    };

    let mut loop_body = vec![];
    if let Some(body) = reaching_body {
        // Discard dom-tree-produced structured forms for body nodes - reaching builds the
        // whole body in one go from the raw input. Post-loop succ nodes (owned_succs that are
        // outside `loop_nodes`) keep their entries; the wrapping logic below still consumes
        // them for the post-loop append / dispatch arm assembly.
        for n in &loop_nodes {
            structured_blocks.remove(n);
        }
        let body_with_breaks = insert_breaks(&loop_nodes, loop_head, loop_successor, body);
        loop_body.push(body_with_breaks);
    } else {
        // Dom-tree body assembly: emit body nodes in RPO over `loop_nodes`. RPO places each
        // absorbed IfElse's post-dominator right after it, so `LatchKind::InLoop` Jumps in
        // `insert_breaks` can be dropped without breaking fall-through.
        let body_order = reverse_post_order_within(&graph.cfg, loop_head, &loop_nodes);
        for node in body_order {
            let Some(node) = structured_blocks.remove(&node) else {
                continue;
            };
            let result = insert_breaks(&loop_nodes, loop_head, loop_successor, node);
            loop_body.push(result);
        }
        // Drop tail-Jumps in body[i] that target body[i+1]'s entry: RPO adjacency makes them
        // jumps to their own fall-through.
        elide_inter_item_gotos(&mut loop_body);
    }

    let (result, absorbed_succs): (D::Structured, HashSet<NodeIndex>) = if multi_successor_mode {
        // Dispatch absorbs every owned succ into the SelectorMatch tail.
        let r = emit_dispatch_arms(graph, structured_blocks, loop_head, &owned_succs, loop_body);
        (r, owned_succs.iter().copied().collect())
    } else {
        let (r, absorbed) = emit_single_exit_loop(
            graph,
            structured_blocks,
            loop_head,
            loop_body,
            loop_successor,
            &owned_succs,
        );
        (r, absorbed.into_iter().collect())
    };
    structured_blocks.insert(loop_head, result);
    install_reduced_marker(input, loop_head, &loop_nodes, &succ_nodes, &absorbed_succs);
}

/// Replace the loop's input with `Input::Reduced(loop_head, succs)` so outer scopes treat it
/// as a single opaque block via `process_reduced`. Drains body members and `absorbed_succs`
/// (succs already embedded into the loop's structured form by `emit_*`), excluding them from
/// the marker's out-edges.
fn install_reduced_marker(
    input: &mut BTreeMap<D::Label, D::Input>,
    loop_head: NodeIndex,
    loop_nodes: &HashSet<NodeIndex>,
    succ_nodes: &HashSet<NodeIndex>,
    absorbed_succs: &HashSet<NodeIndex>,
) {
    for n in loop_nodes {
        if *n != loop_head {
            input.remove(n);
        }
    }
    for s in absorbed_succs {
        input.remove(s);
    }
    let mut succs: Vec<NodeIndex> = succ_nodes
        .iter()
        .copied()
        .filter(|s| !absorbed_succs.contains(s))
        .collect();
    succs.sort_by_key(|n| n.index());
    input.insert(loop_head, D::Input::Reduced(loop_head, succs));
}

/// Multi-succ loop: synthesize a dispatch local `__dispatch_<N>`, rewrite every
/// owned-succ Jump in the loop body to `sel = k; break`, and emit a `match (sel)`
/// after the loop with one arm per owned succ (No More Gotos; Yakdan 2015).
fn emit_dispatch_arms(
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<D::Label, D::Structured>,
    loop_head: NodeIndex,
    owned_succs: &[NodeIndex],
    loop_body: Vec<D::Structured>,
) -> D::Structured {
    use crate::ast::DispatchTag;
    let sel_name = format!("__dispatch_{}", loop_head.index());
    let dispatch_map: HashMap<NodeIndex, DispatchTag> = owned_succs
        .iter()
        .enumerate()
        .map(|(idx, &succ)| (succ, idx as DispatchTag))
        .collect();
    // Dense 0..N-1 tag range is a load-bearing precondition of
    // `inline_dispatch_cascade` (which requires `if (sel <= 0); ...; if (sel <= N-1)`).
    // `.enumerate()` establishes it by construction; the debug_assert catches a future
    // edit that introduces gaps or duplicates.
    debug_assert_eq!(dispatch_map.len(), owned_succs.len());
    debug_assert!(
        (0..owned_succs.len() as DispatchTag).all(|k| dispatch_map.values().any(|&v| v == k)),
        "dispatch tags must be a contiguous 0..N-1 range",
    );
    let mut body_seq = D::Structured::Seq(loop_body);
    rewrite_jumps_for_dispatch(&mut body_seq, &dispatch_map, &sel_name, loop_head);
    graph.update_loop_info(loop_head);
    let loop_expr = D::Structured::Loop(loop_head, Box::new(body_seq));

    // Each arm is the cascade rooted at its owned succ. `cascade_next` maps each succ to the
    // lowest-indexed owned succ reachable via a CFG edge leaving its dom-subtree -- handles
    // succs whose body contains nested structure (e.g. an inner loop). Assumes a linear
    // cascade; non-min forks survive as raw `Jump`s for downstream refinements. Arms clone
    // their tails; `compress_dispatch_cascade` folds the duplication later.
    // TODO: replace with NMG §V-B NCD algorithm using `acyclic::reaching_conditions`.
    let owned_set: HashSet<NodeIndex> = owned_succs.iter().copied().collect();
    let cascade_next: HashMap<NodeIndex, NodeIndex> = owned_succs
        .iter()
        .filter_map(|&s| {
            let subtree: Vec<NodeIndex> = graph
                .dom_tree
                .get(s)
                .all_children()
                .chain(std::iter::once(s))
                .collect();
            let mut exits: Vec<NodeIndex> = subtree
                .iter()
                .flat_map(|n| graph.cfg.neighbors_directed(*n, Direction::Outgoing))
                .filter(|n| !subtree.contains(n) && owned_set.contains(n) && *n != s)
                .collect();
            exits.sort_by_key(|n| n.index());
            exits.dedup();
            exits.into_iter().next().map(|n| (s, n))
        })
        .collect();
    let mut arms: Vec<(DispatchTag, D::Structured)> = Vec::with_capacity(owned_succs.len());
    for (idx, &succ) in owned_succs.iter().enumerate() {
        let (body, _consumed) =
            structure_cascade(succ, structured_blocks, /*consume*/ false, |cur, _| {
                cascade_next.get(&cur).copied()
            });
        arms.push((idx as DispatchTag, body));
    }
    // Owned-succ bodies are cloned (not consumed) per arm; sweep them out of
    // `structured_blocks` now so they don't get double-placed by an outer scope.
    for &s in owned_succs {
        structured_blocks.remove(&s);
    }
    D::Structured::Seq(vec![
        D::Structured::Let(sel_name.clone()),
        loop_expr,
        D::Structured::SelectorMatch(sel_name, arms),
    ])
}

/// Single-owned-succ case: place the owned succ's body after the `Loop` form. Unowned succs
/// (latch-bound to an outer scope) stay in `structured_blocks` for that scope to consume.
/// Returns the absorbed succ (if any) so `install_reduced_marker` can drop its raw `input`
/// entry and exclude it from the `Reduced` out-edges.
fn emit_single_exit_loop(
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<D::Label, D::Structured>,
    loop_head: NodeIndex,
    loop_body: Vec<D::Structured>,
    loop_successor: Option<NodeIndex>,
    owned_succs: &[NodeIndex],
) -> (D::Structured, Option<NodeIndex>) {
    let seq = D::Structured::Seq(loop_body);
    graph.update_loop_info(loop_head);
    let mut result = D::Structured::Loop(loop_head, Box::new(seq));
    let mut absorbed: Option<NodeIndex> = None;
    if let Some(loop_successor) = loop_successor
        && owned_succs.contains(&loop_successor)
        && let Some(succ_structured) = structured_blocks.remove(&loop_successor)
    {
        result = D::Structured::Seq(vec![result, succ_structured]);
        absorbed = Some(loop_successor);
    }
    (result, absorbed)
}

/// Speculatively structure a multi-owned-succ loop as single-exit + sibling-placed succs.
/// Returns `true` on success (caller commits the speculative state). Returns `false` if the
/// body has a residual Jump to a non-primary owned succ -- real per-exit divergence that
/// needs dispatch. On success: `Seq[Loop(body), primary, other_0, ..., other_N]` in CFG RPO.
fn try_structure_loop_without_dispatch(
    ctx: StructureContext<'_>,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    input: &mut BTreeMap<D::Label, D::Input>,
    loop_head: NodeIndex,
    loop_nodes: &HashSet<NodeIndex>,
    succ_nodes: &HashSet<NodeIndex>,
    owned_succs: &[NodeIndex],
    primary: NodeIndex,
    absorbed_out: &mut HashSet<NodeIndex>,
) -> bool {
    let config = ctx.config;
    let terms = ctx.terms;
    let non_primary: HashSet<NodeIndex> = owned_succs
        .iter()
        .copied()
        .filter(|n| *n != primary)
        .collect();

    acyclic::structure_acyclic_node(
        ctx,
        graph,
        structured_blocks,
        loop_head,
        input,
        /*loop_successor*/ Some(primary),
    );

    // Reaching-condition acyclic structuring for the loop body - same hook the single-exit
    // `structure_loop` path uses. We try reaching on the live `input` restricted to
    // `loop_nodes`; inner sub-loops are already `Input::Reduced` markers (post-order DFS).
    // On success the body is one Structured form ready for `insert_breaks` once. On failure
    // we fall through to the dom-tree body assembly below.
    let reaching_body: Option<D::Structured> = {
        let region_input: BTreeMap<D::Label, D::Input> = input
            .iter()
            .filter(|(k, _)| loop_nodes.contains(k))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        let members: HashSet<NodeIndex> = loop_nodes
            .iter()
            .copied()
            .filter(|n| *n != loop_head)
            .collect();
        acyclic::structure_acyclic(
            config,
            terms,
            structured_blocks,
            &region_input,
            loop_head,
            &members,
        )
    };

    let mut loop_body = vec![];
    if let Some(body) = reaching_body {
        for n in loop_nodes {
            structured_blocks.remove(n);
        }
        let body_with_breaks = insert_breaks(loop_nodes, loop_head, Some(primary), body);
        loop_body.push(body_with_breaks);
    } else {
        let body_order = reverse_post_order_within(&graph.cfg, loop_head, loop_nodes);
        for node in body_order {
            let Some(node) = structured_blocks.remove(&node) else {
                continue;
            };
            let result = insert_breaks(loop_nodes, loop_head, Some(primary), node);
            loop_body.push(result);
        }
        elide_inter_item_gotos(&mut loop_body);
    }

    // Check: any residual `Jump`/`JumpIf` targeting a non-primary owned succ means the body
    // has real per-exit divergence we can't express without a selector. Bail to dispatch.
    let body_seq = D::Structured::Seq(loop_body);
    if has_jump_to(&body_seq, &non_primary) {
        return false;
    }
    let D::Structured::Seq(loop_body) = body_seq else {
        unreachable!()
    };

    graph.update_loop_info(loop_head);
    let loop_expr = D::Structured::Loop(loop_head, Box::new(D::Structured::Seq(loop_body)));

    // Place primary first (it's where `Break(loop_head)` lands), then the rest in CFG RPO
    // order so cascade-style chains lay out in flow order and `elide_inter_item_gotos` can
    // drop tail Jumps between adjacent siblings.
    let mut tail = vec![loop_expr];
    let succ_set: HashSet<NodeIndex> = succ_nodes.iter().copied().collect();
    let succ_rpo = reverse_post_order_within(&graph.cfg, primary, &succ_set);
    let mut placed: HashSet<NodeIndex> = HashSet::new();
    for n in std::iter::once(primary).chain(succ_rpo.into_iter()) {
        if !owned_succs.contains(&n) || !placed.insert(n) {
            continue;
        }
        if let Some(body) = structured_blocks.remove(&n) {
            tail.push(body);
            absorbed_out.insert(n);
        }
    }
    elide_inter_item_gotos(&mut tail);

    structured_blocks.insert(loop_head, D::Structured::Seq(tail));
    true
}

/// True if `s` contains any `Jump(_, t)` or `JumpIf(_, _, t1, t2)` with `t`/`t1`/`t2 in
/// targets`. Recurses through every structured form including `Loop` and `Match` arms.
fn has_jump_to(s: &D::Structured, targets: &HashSet<NodeIndex>) -> bool {
    use D::Structured as DS;
    match s {
        DS::Jump(_, t) => targets.contains(t),
        DS::JumpIf(_, _, t1, t2) => targets.contains(t1) || targets.contains(t2),
        DS::Seq(items) => items.iter().any(|i| has_jump_to(i, targets)),
        DS::CondIf(_, c, a) => {
            has_jump_to(c, targets)
                || a.as_ref()
                    .as_ref()
                    .is_some_and(|alt| has_jump_to(alt, targets))
        }
        DS::Switch(_, _, arms) => arms.iter().any(|(_, b)| has_jump_to(b, targets)),
        DS::SelectorMatch(_, arms) => arms.iter().any(|(_, b)| has_jump_to(b, targets)),
        DS::Loop(_, body) => has_jump_to(body, targets),
        DS::Break(_) | DS::Continue(_) | DS::Block(_) | DS::Let(_) | DS::Assign(_, _) => false,
    }
}

/// In dispatch mode, walk `s` and rewrite each jump in our dispatch map to instead exit
/// to the loop node with the appropriate dispatch flag set.
fn rewrite_jumps_for_dispatch(
    s: &mut D::Structured,
    dispatch_map: &HashMap<NodeIndex, crate::ast::DispatchTag>,
    sel_name: &str,
    loop_head: NodeIndex,
) {
    use D::Structured as DS;
    let dispatch_for = |target: NodeIndex| -> Option<DS> {
        dispatch_map.get(&target).map(|&tag| {
            DS::Seq(vec![
                DS::Assign(sel_name.to_string(), tag),
                DS::Break(loop_head),
            ])
        })
    };
    match s {
        DS::Jump(_, target) => {
            if let Some(replacement) = dispatch_for(*target) {
                *s = replacement;
            }
        }
        DS::JumpIf(src, code, then_target, else_target) => {
            let then_dispatch = dispatch_for(*then_target);
            let else_dispatch = dispatch_for(*else_target);
            if then_dispatch.is_some() || else_dispatch.is_some() {
                let then_body = then_dispatch.unwrap_or(DS::Jump(*src, *then_target));
                let else_body = else_dispatch.unwrap_or(DS::Jump(*src, *else_target));
                *s = DS::CondIf(
                    predicates::cond_atom(*code),
                    Box::new(then_body),
                    Box::new(Some(else_body)),
                );
            }
        }
        DS::Seq(items) => {
            for item in items.iter_mut() {
                rewrite_jumps_for_dispatch(item, dispatch_map, sel_name, loop_head);
            }
        }
        DS::CondIf(_, conseq, alt) => {
            rewrite_jumps_for_dispatch(conseq, dispatch_map, sel_name, loop_head);
            if let Some(alt_inner) = alt.as_mut().as_mut() {
                rewrite_jumps_for_dispatch(alt_inner, dispatch_map, sel_name, loop_head);
            }
        }
        DS::Switch(_, _, cases) => {
            for (_, body) in cases.iter_mut() {
                rewrite_jumps_for_dispatch(body, dispatch_map, sel_name, loop_head);
            }
        }
        DS::Loop(_, body) => {
            rewrite_jumps_for_dispatch(body, dispatch_map, sel_name, loop_head);
        }
        DS::Block(_)
        | DS::Break(_)
        | DS::Continue(_)
        | DS::Let(_)
        | DS::Assign(_, _)
        | DS::SelectorMatch(_, _) => {}
    }
}

pub(super) fn insert_breaks(
    loop_nodes: &HashSet<NodeIndex>,
    loop_head: NodeIndex,
    loop_successor: Option<NodeIndex>,
    node: D::Structured,
) -> D::Structured {
    use D::Structured as DS;

    // Classification of a Jump/JumpIf target relative to *this* loop only. Anything that targets
    // an enclosing loop will be a raw `DS::Jump(target)` at this layer and gets reclassified
    // when the next-outer `structure_loop` runs `insert_breaks` and recurs into our `Loop`
    // body - `Latch` is the fall-through that keeps the jump intact for that later pass.
    enum LatchKind {
        Continue,
        Break,
        InLoop,
        Latch,
    }

    fn find_latch_kind(
        loop_nodes: &HashSet<NodeIndex>,
        loop_head: NodeIndex,
        loop_successor: Option<NodeIndex>,
        node_ndx: NodeIndex,
    ) -> LatchKind {
        if node_ndx == loop_head {
            LatchKind::Continue
        } else if Some(node_ndx) == loop_successor {
            LatchKind::Break
        } else if loop_nodes.contains(&node_ndx) {
            LatchKind::InLoop
        } else {
            LatchKind::Latch
        }
    }

    fn lower_conseq(latch: LatchKind, loop_head: NodeIndex, next: NodeIndex) -> Box<DS> {
        let conseq = match latch {
            LatchKind::Continue => DS::Continue(loop_head),
            LatchKind::Break => DS::Break(loop_head),
            LatchKind::InLoop => DS::Seq(vec![]),
            // A JumpIf arm that escapes this loop (and isn't a body fall-through).
            // Tagged D2 because it originates inside `insert_breaks`'s JumpIf handling.
            LatchKind::Latch => DS::Jump(GotoSource::EscapeJumpIf, next),
        };
        Box::new(conseq)
    }

    fn lower_alt(latch: LatchKind, loop_head: NodeIndex, next: NodeIndex) -> Box<Option<DS>> {
        if matches!(latch, LatchKind::InLoop) {
            Box::new(None)
        } else {
            Box::new(Some(*lower_conseq(latch, loop_head, next)))
        }
    }

    match node {
        DS::Block(_) => node,
        DS::Seq(nodes) => DS::Seq(
            nodes
                .into_iter()
                .map(|node| insert_breaks(loop_nodes, loop_head, loop_successor, node))
                .collect::<Vec<_>>(),
        ),
        // Already-labeled Break/Continue (emitted by a nested loop's earlier insert_breaks)
        // target some inner loop, not this one - pass through unchanged.
        DS::Break(_) | DS::Continue(_) => node,
        DS::CondIf(cond, conseq, alt) => DS::CondIf(
            cond,
            Box::new(insert_breaks(
                loop_nodes,
                loop_head,
                loop_successor,
                *conseq,
            )),
            Box::new(alt.map(|alt| insert_breaks(loop_nodes, loop_head, loop_successor, alt))),
        ),
        DS::Jump(src, next) => match find_latch_kind(loop_nodes, loop_head, loop_successor, next) {
            LatchKind::Continue => DS::Continue(loop_head),
            LatchKind::Break => DS::Break(loop_head),
            // TODO check if jump target is the next node
            LatchKind::InLoop => D::Structured::Seq(vec![]),
            // Targets neither this loop nor anything dominated by it; leave the raw Jump for
            // an enclosing loop's pass (or for `generate_output` to lower to Unstructured).
            // Preserve the original creation tag: the same Jump escaping outward.
            LatchKind::Latch => DS::Jump(src, next),
        },
        DS::JumpIf(src, code, next, other) => {
            let next_latch = find_latch_kind(loop_nodes, loop_head, loop_successor, next);
            let other_latch = find_latch_kind(loop_nodes, loop_head, loop_successor, other);
            match (next_latch, other_latch) {
                (LatchKind::Continue, LatchKind::Continue) => DS::Continue(loop_head),
                (LatchKind::Break, LatchKind::Break) => DS::Break(loop_head),
                (LatchKind::Latch, LatchKind::Latch) => DS::JumpIf(src, code, next, other),
                (LatchKind::InLoop, LatchKind::InLoop) => unreachable!(),
                (conseq_lk, alt_lk) => {
                    let conseq = lower_conseq(conseq_lk, loop_head, next);
                    let alt = lower_alt(alt_lk, loop_head, other);
                    DS::CondIf(predicates::cond_atom(code), conseq, alt)
                }
            }
        }
        DS::Loop(label, structured) => DS::Loop(
            label,
            Box::new(insert_breaks(
                loop_nodes,
                loop_head,
                loop_successor,
                *structured,
            )),
        ),
        DS::Switch(code, enum_, structureds) => {
            let result = structureds
                .into_iter()
                .map(|(v, structured)| {
                    (
                        v,
                        insert_breaks(loop_nodes, loop_head, loop_successor, structured),
                    )
                })
                .collect::<Vec<_>>();
            DS::Switch(code, enum_, result)
        }
        DS::Let(_) | DS::Assign(_, _) => node,
        DS::SelectorMatch(name, arms) => DS::SelectorMatch(
            name,
            arms.into_iter()
                .map(|(tag, body)| {
                    (
                        tag,
                        insert_breaks(loop_nodes, loop_head, loop_successor, body),
                    )
                })
                .collect(),
        ),
    }
}

// -------------------------------------------------------------------------------------------------
// Cascade helper (used by emit_dispatch_arms)
// -------------------------------------------------------------------------------------------------

/// Cascade `start` through `source`, asking `step` for the next node to fold at each step.
/// `consume=true` removes bodies; `consume=false` clones them (dispatch arms need their own
/// tail copy for `compress_dispatch_cascade` to fold back). Returns the cascade body and the
/// consumed set. Adjacent items become fall-through neighbors via `elide_inter_item_gotos`.
fn structure_cascade(
    start: NodeIndex,
    source: &mut BTreeMap<NodeIndex, D::Structured>,
    consume: bool,
    mut step: impl FnMut(NodeIndex, &D::Structured) -> Option<NodeIndex>,
) -> (D::Structured, HashSet<NodeIndex>) {
    let mut chain: Vec<D::Structured> = Vec::new();
    let mut consumed: HashSet<NodeIndex> = HashSet::new();
    let mut cursor = start;

    loop {
        let body = if consume {
            source.remove(&cursor)
        } else {
            source.get(&cursor).cloned()
        };
        let Some(body) = body else { break };
        let next = step(cursor, &body);
        consumed.insert(cursor);
        chain.push(body);

        let Some(next) = next else { break };
        if consumed.contains(&next) {
            break;
        }
        if !source.contains_key(&next) {
            break;
        }
        cursor = next;
    }

    // Adjacent items are now structural fall-through neighbors.
    elide_inter_item_gotos(&mut chain);
    (D::Structured::seq_or_singleton(chain), consumed)
}

// -------------------------------------------------------------------------------------------------
// Local CFG helpers
// -------------------------------------------------------------------------------------------------

/// Reverse-post-order DFS over `cfg`, restricted to nodes in `members`, rooted at `root`.
/// Successor edges that leave `members` are not followed; nodes not reachable from `root`
/// through `members` are omitted. Returns nodes in RPO (a topological order on the DAG-like
/// projection of the CFG inside `members`).
fn reverse_post_order_within(
    cfg: &petgraph::Graph<(), ()>,
    root: NodeIndex,
    members: &HashSet<NodeIndex>,
) -> Vec<NodeIndex> {
    fn visit(
        cfg: &petgraph::Graph<(), ()>,
        node: NodeIndex,
        members: &HashSet<NodeIndex>,
        visited: &mut HashSet<NodeIndex>,
        post: &mut Vec<NodeIndex>,
    ) {
        if !members.contains(&node) || !visited.insert(node) {
            return;
        }
        for succ in cfg.neighbors(node) {
            visit(cfg, succ, members, visited, post);
        }
        post.push(node);
    }
    let mut visited = HashSet::new();
    let mut post = Vec::new();
    visit(cfg, root, members, &mut visited, &mut post);
    post.reverse();
    post
}

// -------------------------------------------------------------------------------------------------
// Inter-item Jump elision (loop body assembly + cascade)
// -------------------------------------------------------------------------------------------------

/// The CFG node id this structured form starts execution at, when one is well-defined.
/// Returns `None` for forms with no single entry (empty Seq, Continue/Break, raw Jumps).
fn entry_label(s: &D::Structured) -> Option<NodeIndex> {
    use D::Structured as DS;
    match s {
        DS::Block(code) => Some(NodeIndex::new(*code as usize)),
        DS::Switch(code, _, _) => Some(NodeIndex::new(*code as usize)),
        DS::JumpIf(_, code, _, _) => Some(NodeIndex::new(*code as usize)),
        DS::Loop(label, _) => Some(*label),
        DS::Seq(items) => items.iter().find_map(entry_label),
        DS::Break(_) | DS::Continue(_) | DS::Jump(_, _) => None,
        // A single-atom `CondIf` (the dom-tree structurer's product) has the atom's block
        // as its CFG entry. A compound-formula `CondIf` is a recovered boolean over multiple
        // condition blocks - no single entry.
        DS::CondIf(cond, _, _) => cond.as_cond_atom(),
        // Dispatch synthesis nodes carry no CFG entry of their own.
        DS::Let(_) | DS::Assign(_, _) | DS::SelectorMatch(_, _) => None,
    }
}

/// Drop any `Jump(_, target)` sitting at a tail position of `s`. Walks through `Seq`'s last
/// item, both `IfElse` arms, and every `Switch` arm. Doesn't descend into `Loop` bodies -
/// they don't fall through. When the tail Jump is the last item of a `Seq`, pop it (rather
/// than replacing with an empty `Seq`) so we don't leave stray empties behind.
fn elide_tail_jump_to(s: &mut D::Structured, target: NodeIndex) {
    use D::Structured as DS;
    match s {
        DS::Jump(_, label) if *label == target => {
            *s = DS::Seq(vec![]);
        }
        DS::Seq(items) => {
            if matches!(items.last(), Some(DS::Jump(_, label)) if *label == target) {
                items.pop();
            } else if let Some(last) = items.last_mut() {
                elide_tail_jump_to(last, target);
            }
        }
        DS::CondIf(_, conseq, alt) => {
            elide_tail_jump_to(conseq, target);
            if let Some(alt_inner) = alt.as_mut().as_mut() {
                elide_tail_jump_to(alt_inner, target);
            }
        }
        DS::Switch(_, _, cases) => {
            for (_, body) in cases.iter_mut() {
                elide_tail_jump_to(body, target);
            }
        }
        DS::Block(_)
        | DS::Loop(_, _)
        | DS::Break(_)
        | DS::Continue(_)
        | DS::Jump(_, _)
        | DS::JumpIf(_, _, _, _)
        | DS::Let(_)
        | DS::Assign(_, _)
        | DS::SelectorMatch(_, _) => {}
    }
}

/// Used only by `structure_loop`'s body assembly: body[i]'s next sibling is body[i+1], and
/// neither was structured with that knowledge. For every consecutive pair, walk `items[i]`'s
/// tails and drop a `Jump` whose target is `items[i+1]`'s entry label.
fn elide_inter_item_gotos(items: &mut [D::Structured]) {
    for i in 0..items.len().saturating_sub(1) {
        if let Some(next_label) = entry_label(&items[i + 1]) {
            let (left, _) = items.split_at_mut(i + 1);
            elide_tail_jump_to(&mut left[i], next_label);
        }
    }
}

// -------------------------------------------------------------------------------------------------
// NMG §V-B helpers
// -------------------------------------------------------------------------------------------------
// Compute reaching condition formulas for each owned succ, so a downstream cascade can
// replace the dispatch table. The body's acyclic projection drops back-edges to
// `loop_head` (they go to a synthetic sink) and adds `owned_succs` as terminal sinks --
// `acyclic::reaching_conditions` then produces `c(loop_head, succ)` for every owned succ.
// See `V_B_PLAN.md` for the full plan.

/// Compute `c(loop_head, succ)` for each `owned_succ`. Returns `None` if the body region
/// has shapes `reaching_conditions` can't handle (e.g. `Variants`).
#[allow(dead_code)] // wired in by the cascade emitter once landed
pub(super) fn compute_owned_succ_formulas(
    input: &BTreeMap<D::Label, D::Input>,
    loop_nodes: &HashSet<NodeIndex>,
    owned_succs: &[NodeIndex],
    loop_head: NodeIndex,
) -> Option<BTreeMap<NodeIndex, Formula>> {
    let projection = build_body_acyclic_projection(input, loop_nodes, owned_succs, loop_head);
    let reach = acyclic::reaching_conditions(&projection, loop_head)?;
    let mut out = BTreeMap::new();
    for &s in owned_succs {
        out.insert(s, reach.get(&s).cloned()?);
    }
    Some(out)
}

/// Build the body's acyclic projection: input restricted to `loop_nodes`, with back-edges
/// to `loop_head` redirected to a synthetic sink, plus `owned_succs` added as sinks.
fn build_body_acyclic_projection(
    input: &BTreeMap<D::Label, D::Input>,
    loop_nodes: &HashSet<NodeIndex>,
    owned_succs: &[NodeIndex],
    loop_head: NodeIndex,
) -> BTreeMap<D::Label, D::Input> {
    let back_edge_sink = synthetic_sink_id(input);
    let mut projection: BTreeMap<D::Label, D::Input> = BTreeMap::new();
    for (&node, inp) in input.iter().filter(|(k, _)| loop_nodes.contains(k)) {
        projection.insert(node, redirect_edges_to(inp.clone(), loop_head, back_edge_sink));
    }
    // Synthetic sink for the back-edges.
    projection.insert(back_edge_sink, D::Input::Code(back_edge_sink, 0, None));
    // Owned succs as terminal sinks. Their actual `input` entries live outside `loop_nodes`
    // and may carry their own out-edges, but for reaching-condition purposes inside the body
    // we just need them as nodes that receive flow.
    for &succ in owned_succs {
        projection.insert(succ, D::Input::Code(succ, 0, None));
    }
    projection
}

/// Allocate a `NodeIndex` past anything currently in `input`. Used to slot synthetic
/// projection sinks without colliding with real node ids.
fn synthetic_sink_id(input: &BTreeMap<D::Label, D::Input>) -> NodeIndex {
    let max = input.keys().map(|n| n.index()).max().unwrap_or(0);
    NodeIndex::new(max + 1)
}

/// Rewrite every target node in `inp` equal to `from` so it points at `to` instead.
fn redirect_edges_to(inp: D::Input, from: NodeIndex, to: NodeIndex) -> D::Input {
    let remap = |n: NodeIndex| if n == from { to } else { n };
    match inp {
        D::Input::Condition(l, c, then, els) => D::Input::Condition(l, c, remap(then), remap(els)),
        D::Input::Variants(l, c, e, items) => D::Input::Variants(
            l,
            c,
            e,
            items.into_iter().map(|(v, t)| (v, remap(t))).collect(),
        ),
        D::Input::Code(l, c, Some(next)) => D::Input::Code(l, c, Some(remap(next))),
        D::Input::Code(l, c, None) => D::Input::Code(l, c, None),
        D::Input::Reduced(l, succs) => {
            D::Input::Reduced(l, succs.into_iter().map(remap).collect())
        }
    }
}
