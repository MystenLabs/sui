// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Cyclic-region structuring.
//!
//! Per loop head identified by `Graph::loop_heads`, run `structure_loop`. It structures
//! the body via NMG (`acyclic::structure_region`), places post-loop bodies, and
//! synthesizes a dispatch table on multi-owned-succ shapes. On completion, an
//! `Input::Reduced(loop_head, succs)` marker replaces the body in `input` so outer scopes
//! treat the loop as an opaque NMG IV-C abstract node.

use crate::structuring::{
    StructureContext, acyclic,
    ast::{self as D},
    graph::Graph,
    region::SinkRendering,
};

use petgraph::{Direction, graph::NodeIndex};

use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) fn structure_loop(
    ctx: StructureContext<'_>,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    loop_head: NodeIndex,
    input: &mut BTreeMap<D::Label, D::Input>,
    unstructured: &mut HashSet<u64>,
) {
    let config = ctx.config;
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

        let mut spec_unstructured = unstructured.clone();
        if try_structure_loop_without_dispatch(
            &mut spec_graph,
            &mut spec_blocks,
            &mut spec_input,
            loop_head,
            &loop_nodes,
            &succ_nodes,
            &owned_succs,
            primary,
            &mut spec_absorbed,
            &mut spec_unstructured,
        ) {
            *unstructured = spec_unstructured;
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

    // NMG structures the loop body in one go from the raw `input`, restricted to
    // `loop_nodes`. `members = loop_nodes \ {loop_head}` makes back-edges to `loop_head`
    // fire the out-of-region rule, emitting exit-jumps that `insert_breaks` rewrites to
    // `Continue`. Inner sub-loops are already `Input::Reduced` markers (post-order DFS).
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
    let body = acyclic::structure_region(
        structured_blocks,
        &region_input,
        loop_head,
        &members,
        SinkRendering::Loop,
        unstructured,
    )
    .expect("NMG failed on loop body");
    let body_with_breaks = insert_breaks(&loop_nodes, loop_head, loop_successor, body);
    let loop_body = vec![body_with_breaks];

    let (result, absorbed_succs): (D::Structured, HashSet<NodeIndex>) = if multi_successor_mode {
        // Dispatch absorbs every owned succ into the SelectorMatch tail. Each owned succ
        // needs a `structured_blocks` entry for `structure_cascade` to find - seed raw input
        // nodes (Code/Condition/Variants that the structurer never processed into a Reduced
        // marker) with `Block(code)` so they don't silently drop out of the arm body.
        for &s in &owned_succs {
            if structured_blocks.contains_key(&s) {
                continue;
            }
            let block = match input.get(&s) {
                Some(D::Input::Code(_, code, _))
                | Some(D::Input::Condition(_, code, _, _))
                | Some(D::Input::Variants(_, code, _, _)) => {
                    unstructured.remove(code);
                    D::Structured::Block(*code)
                }
                _ => continue,
            };
            structured_blocks.insert(s, block);
        }
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
/// the marker's out-edges. Each absorbed succ's own out-edges are inherited into the marker
/// so the post-absorbed continuation stays reachable: when the absorbed succ is itself a
/// `Reduced(_, [post])` (an earlier nested loop), `post` becomes a successor of the new
/// marker. Without this, the chain of blocks downstream of the absorbed succ goes orphan in
/// the residue and silently drops out.
fn install_reduced_marker(
    input: &mut BTreeMap<D::Label, D::Input>,
    loop_head: NodeIndex,
    loop_nodes: &HashSet<NodeIndex>,
    succ_nodes: &HashSet<NodeIndex>,
    absorbed_succs: &HashSet<NodeIndex>,
) {
    let inherited_succs: HashSet<NodeIndex> = absorbed_succs
        .iter()
        .filter_map(|s| input.get(s))
        .flat_map(|inp| inp.edges().into_iter().map(|(_, v)| v))
        .collect();
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
        .chain(inherited_succs)
        .filter(|s| !absorbed_succs.contains(s) && !loop_nodes.contains(s))
        .collect();
    succs.sort_by_key(|n| n.index());
    succs.dedup();
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
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    input: &mut BTreeMap<D::Label, D::Input>,
    loop_head: NodeIndex,
    loop_nodes: &HashSet<NodeIndex>,
    succ_nodes: &HashSet<NodeIndex>,
    owned_succs: &[NodeIndex],
    primary: NodeIndex,
    absorbed_out: &mut HashSet<NodeIndex>,
    unstructured: &mut HashSet<u64>,
) -> bool {
    let non_primary: HashSet<NodeIndex> = owned_succs
        .iter()
        .copied()
        .filter(|n| *n != primary)
        .collect();

    // NMG structures the loop body in one go. Same hook the main `structure_loop`
    // path uses; inner sub-loops are already `Input::Reduced` markers.
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
    let body = acyclic::structure_region(
        structured_blocks,
        &region_input,
        loop_head,
        &members,
        SinkRendering::Loop,
        unstructured,
    )
    .expect("NMG failed on speculative multi-succ loop body");
    let body_with_breaks = insert_breaks(loop_nodes, loop_head, Some(primary), body);
    let loop_body = vec![body_with_breaks];

    // Check: any residual `Jump` targeting a non-primary owned succ means the body has
    // real per-exit divergence we can't express without a selector. Bail to dispatch.
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
    for n in std::iter::once(primary).chain(succ_rpo) {
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

/// True if `s` contains any `Jump(_, t)` with `t in targets`. Recurs through every
/// structured form including `Loop` and `Match` arms.
fn has_jump_to(s: &D::Structured, targets: &HashSet<NodeIndex>) -> bool {
    use D::Structured as DS;
    match s {
        DS::Jump(_, t) => targets.contains(t),
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
        DS::Break(_) | DS::Continue(_) | DS::Block(_) | DS::Let(_) | DS::AssignTag(_, _) => false,
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
                DS::AssignTag(sel_name.to_string(), tag),
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
        | DS::AssignTag(_, _)
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
        DS::Let(_) | DS::AssignTag(_, _) => node,
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
        DS::Loop(label, _) => Some(*label),
        DS::Seq(items) => items.iter().find_map(entry_label),
        DS::Break(_) | DS::Continue(_) | DS::Jump(_, _) => None,
        // Entry label available iff the `cond` is an Atom, otherwise it is the result of
        // multiple condition blocks built/nested.
        DS::CondIf(cond, _, _) => cond.as_cond_atom(),
        // Dispatch synthesis nodes carry no CFG entry of their own.
        DS::Let(_) | DS::AssignTag(_, _) | DS::SelectorMatch(_, _) => None,
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
        | DS::Let(_)
        | DS::AssignTag(_, _)
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
