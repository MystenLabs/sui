// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod ast;
pub(crate) mod dom_tree;
pub(crate) mod graph;
pub(crate) mod hoist_declarations;
pub(crate) mod term_reconstruction;

use crate::{
    config::{self, print_heading},
    structuring::{
        ast::{self as D, GotoSource},
        graph::Graph,
    },
};

use petgraph::{Direction, graph::NodeIndex, visit::DfsPostOrder};

use std::collections::{BTreeMap, HashMap, HashSet};

// ------------------------------------------------------------------------------------------------
// Structuring Algorithm
// ------------------------------------------------------------------------------------------------
// This algorithm is (loosely) based on No More Gotos (2015), with a number of modifications to
// make it Move-specific. Part of the change also includes leveraging what we know about Move
// compilation to avoid some of the more-complex structuring issues that arise in general
// decompilation.

pub(crate) fn structure(
    config: &config::Config,
    mut input: BTreeMap<D::Label, D::Input>,
    entry_node: D::Label,
) -> (D::Structured, Vec<u64>) {
    // Native functions have empty basic blocks - return early to avoid panicking in Graph::new
    if input.is_empty() {
        return (D::Structured::Seq(vec![]), vec![]);
    }

    let mut graph = Graph::new(config, &input, entry_node);
    // Capture node ids up front — `structure_nodes` drains `input` as it processes each
    // node, so by the time we report `unemitted` the map is empty.
    let all_nodes: Vec<NodeIndex> = input.keys().copied().collect();

    let mut structured_blocks: BTreeMap<D::Label, D::Structured> = BTreeMap::new();

    if config.debug_print.structuring {
        let mut post_order = DfsPostOrder::new(&graph.cfg, entry_node);
        print_heading("post-order traversal");
        println!("cfg: {:#?}", graph.cfg);
        while let Some(node) = post_order.next(&graph.cfg) {
            print!("{:?}  ", node.index());
        }
        println!();
    }

    structure_nodes(
        config,
        &mut input,
        entry_node,
        &mut graph,
        &mut structured_blocks,
    );

    let mut result = structured_blocks.remove(&entry_node).unwrap();
    flatten_sequence(&mut result);
    let unemitted = graph.unemitted_from(&all_nodes);
    (result, unemitted)
}

// -------------------------------------------------------------------------------------------------
// Owned-children hoist
// -------------------------------------------------------------------------------------------------
// `structure_acyclic_region` builds an IfElse/Switch by absorbing each arm target that's an
// immediate dom-tree child of the conditional, and emitting `Jump` for arms targeting the
// post-dominator (the convergence point) or arms whose target lies outside the dom subtree.
// After building the IfElse/Switch, any *other* `ichildren` of the conditional that are
// still in `structured_blocks` are "orphans" — every CFG path to them goes through the
// conditional, so they semantically belong in our sequence. We append them and walk the
// IfElse/Switch's tail positions to drop `Jump`s targeting the now-adjacent block.
//
// Each structurer makes its decisions locally using `ichildren`, the immediate
// post-dominator, and the optional `loop_succ` for loop-head calls — no threaded
// next-sibling context. Loop-body RPO adjacency (body[i]'s next is body[i+1]) is handled
// separately in `structure_loop`'s body assembly via pairwise `elide_tail_jump_to`.

/// The CFG node id this structured form starts execution at, when one is well-defined.
/// Returns `None` for forms with no single entry (empty Seq, Continue/Break, raw Jumps).
fn entry_label(s: &D::Structured) -> Option<NodeIndex> {
    use D::Structured as DS;
    match s {
        DS::Block(code) => Some(NodeIndex::new(*code as usize)),
        DS::IfElse(code, _, _) => Some(NodeIndex::new(*code as usize)),
        DS::Switch(code, _, _) => Some(NodeIndex::new(*code as usize)),
        DS::JumpIf(_, code, _, _) => Some(NodeIndex::new(*code as usize)),
        DS::Loop(label, _) => Some(*label),
        DS::Seq(items) => items.iter().find_map(entry_label),
        DS::Break(_) | DS::Continue(_) | DS::Jump(_, _) => None,
    }
}

/// Drop any `Jump(_, target)` sitting at a tail position of `s`. Walks through `Seq`'s last
/// item, both `IfElse` arms, and every `Switch` arm. Doesn't descend into `Loop` bodies —
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
        DS::IfElse(_, conseq, alt) => {
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
        | DS::JumpIf(_, _, _, _) => {}
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

fn structure_nodes(
    config: &config::Config,
    input: &mut BTreeMap<NodeIndex, ast::Input>,
    entry_node: NodeIndex,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, ast::Structured>,
) {
    let mut post_order = DfsPostOrder::new(&graph.cfg, entry_node);

    while let Some(node) = post_order.next(&graph.cfg) {
        if config.debug_print.structuring {
            println!("Trying to structure node {node:#?}");
            println!("  > cur blocks: {:?}", structured_blocks.keys());
        }
        if graph.loop_heads.contains(&node) {
            structure_loop(config, graph, structured_blocks, node, input);
        } else {
            structure_acyclic(
                config,
                graph,
                structured_blocks,
                node,
                input,
                /*loop_succ*/ None,
            );
        }
    }
}

#[allow(clippy::expect_fun_call)]
fn structure_loop(
    config: &config::Config,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    loop_head: NodeIndex,
    input: &mut BTreeMap<D::Label, D::Input>,
) {
    if config.debug_print.structuring {
        println!("structuring loop at node {loop_head:#?}");
    }
    let (loop_nodes, succ_nodes) = graph.find_loop_nodes(loop_head);
    // succ_nodes is a HashSet; take the lowest index for determinism.
    let succ_node = succ_nodes.iter().copied().min();
    if config.debug_print.structuring {
        println!("  loop nodes: {loop_nodes:#?}");
        println!("  successor nodes: {succ_nodes:#?}");
    }
    structure_acyclic(
        config,
        graph,
        structured_blocks,
        loop_head,
        input,
        /*loop_succ*/ succ_node,
    );

    // Emit body nodes in reverse post-order restricted to `loop_nodes`. After
    // `structure_acyclic_region` has absorbed arm children into structured IfElse/Switch
    // nodes, the only Seq-level siblings of an in-body IfElse are nodes the IfElse does
    // not structurally contain, and RPO places the IfElse's post-dominator immediately
    // after them. That adjacency is what makes the `LatchKind::InLoop` Jump drop in
    // `insert_breaks` sound: falling through goes to the right node.
    //
    // Sort order over NodeIndex would only give the same answer when the upstream
    // bytecode happens to lay blocks out in CFG-flow order, which is true today but
    // isn't an asserted invariant.
    let body_order = reverse_post_order_within(&graph.cfg, loop_head, &loop_nodes);

    let mut loop_body = vec![];
    for node in body_order {
        let Some(node) = structured_blocks.remove(&node) else {
            continue;
        };
        let result = insert_breaks(&loop_nodes, loop_head, succ_node, node);
        loop_body.push(result);
    }
    // Drop tail-Jumps in body[i] that target body[i+1]'s entry: RPO adjacency makes them
    // jumps to their own fall-through.
    elide_inter_item_gotos(&mut loop_body);

    let seq = D::Structured::Seq(loop_body);
    graph.update_loop_info(loop_head);
    let mut result = D::Structured::Loop(loop_head, Box::new(seq));
    if let Some(succ_node) = succ_node
        && graph
            .dom_tree
            .get(loop_head)
            .all_children()
            .any(|child| child == succ_node)
    {
        if let Some(succ_structured) = structured_blocks.remove(&succ_node) {
            result = D::Structured::Seq(vec![result, succ_structured]);
        } else if config.debug_print.structuring {
            println!("  failed to find successor node {succ_node:?} in structured blocks");
        }
    }
    structured_blocks.insert(loop_head, result);
}

fn insert_breaks(
    loop_nodes: &HashSet<NodeIndex>,
    loop_head: NodeIndex,
    succ_node: Option<NodeIndex>,
    node: D::Structured,
) -> D::Structured {
    use D::Structured as DS;

    // Classification of a Jump/JumpIf target relative to *this* loop only. Anything that targets
    // an enclosing loop will be a raw `DS::Jump(target)` at this layer and gets reclassified
    // when the next-outer `structure_loop` runs `insert_breaks` and recurs into our `Loop`
    // body — `Latch` is the fall-through that keeps the jump intact for that later pass.
    enum LatchKind {
        Continue,
        Break,
        InLoop,
        Latch,
    }

    fn find_latch_kind(
        loop_nodes: &HashSet<NodeIndex>,
        loop_head: NodeIndex,
        succ_node: Option<NodeIndex>,
        node_ndx: NodeIndex,
    ) -> LatchKind {
        if node_ndx == loop_head {
            LatchKind::Continue
        } else if Some(node_ndx) == succ_node {
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
                .map(|node| insert_breaks(loop_nodes, loop_head, succ_node, node))
                .collect::<Vec<_>>(),
        ),
        // Already-labeled Break/Continue (emitted by a nested loop's earlier insert_breaks)
        // target some inner loop, not this one — pass through unchanged.
        DS::Break(_) | DS::Continue(_) => node,
        DS::IfElse(code, conseq, alt) => DS::IfElse(
            code,
            Box::new(insert_breaks(loop_nodes, loop_head, succ_node, *conseq)),
            Box::new(alt.map(|alt| insert_breaks(loop_nodes, loop_head, succ_node, alt))),
        ),
        DS::Jump(src, next) => match find_latch_kind(loop_nodes, loop_head, succ_node, next) {
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
            let next_latch = find_latch_kind(loop_nodes, loop_head, succ_node, next);
            let other_latch = find_latch_kind(loop_nodes, loop_head, succ_node, other);
            match (next_latch, other_latch) {
                (LatchKind::Continue, LatchKind::Continue) => DS::Continue(loop_head),
                (LatchKind::Break, LatchKind::Break) => DS::Break(loop_head),
                (LatchKind::Latch, LatchKind::Latch) => DS::JumpIf(src, code, next, other),
                (LatchKind::InLoop, LatchKind::InLoop) => unreachable!(),
                (conseq_lk, alt_lk) => {
                    let conseq = lower_conseq(conseq_lk, loop_head, next);
                    let alt = lower_alt(alt_lk, loop_head, other);
                    DS::IfElse(code, conseq, alt)
                }
            }
        }
        DS::Loop(label, structured) => DS::Loop(
            label,
            Box::new(insert_breaks(loop_nodes, loop_head, succ_node, *structured)),
        ),
        DS::Switch(code, enum_, structureds) => {
            let result = structureds
                .into_iter()
                .map(|(v, structured)| {
                    (
                        v,
                        insert_breaks(loop_nodes, loop_head, succ_node, structured),
                    )
                })
                .collect::<Vec<_>>();
            DS::Switch(code, enum_, result)
        }
    }
}

fn structure_acyclic(
    config: &config::Config,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    node: NodeIndex,
    input: &mut BTreeMap<D::Label, D::Input>,
    loop_succ: Option<NodeIndex>,
) {
    if graph.back_edges.contains_key(&node) {
        let result = structure_latch_node(config, graph, node, input.remove(&node).unwrap());
        structured_blocks.insert(node, result);
    } else {
        let result =
            structure_acyclic_region(config, graph, structured_blocks, input, node, loop_succ);
        structured_blocks.insert(node, result);
    }
}

/// A node is "singly entered" if exactly one of its CFG predecessors lies outside its own
/// dom subtree (including itself). Predecessors inside the subtree are back-edges from a
/// contained loop's latch — they don't represent independent entry into the scope. The
/// `target` itself is part of the subtree so a self-loop's self-edge counts as a back-edge.
/// This is the criterion `arm_for` uses to decide whether an arm body owns its target
/// outright (embed) or whether the target is a shared join point that other paths also
/// reach (sibling-hoist).
fn is_singly_entered(
    target: NodeIndex,
    cfg: &petgraph::graph::DiGraph<(), ()>,
    dom_tree: &dom_tree::DominatorTree,
) -> bool {
    let subtree: HashSet<NodeIndex> = dom_tree
        .get(target)
        .all_children()
        .chain(std::iter::once(target))
        .collect();
    cfg.neighbors_directed(target, Direction::Incoming)
        .filter(|pred| !subtree.contains(pred))
        .count()
        == 1
}

fn structure_acyclic_region(
    config: &config::Config,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    input: &mut BTreeMap<D::Label, D::Input>,
    start: NodeIndex,
    loop_succ: Option<NodeIndex>,
) -> D::Structured {
    // Code has no arms and no post-dominator role; delegate before the dom-tree work.
    if matches!(&input[&start], D::Input::Code(..)) {
        let code_node = input.remove(&start).unwrap();
        return structure_code_node(config, graph, structured_blocks, start, code_node);
    }

    let dom_node = graph.dom_tree.get(start);
    let ichildren = dom_node.immediate_children().collect::<HashSet<_>>();

    if config.debug_print.structuring {
        println!("structuring acyclic region at node {start:#?}");
        println!("  blocks: {structured_blocks:#?}");
        println!("  immediate children: {ichildren:#?}");
        if let Some(s) = loop_succ {
            println!("  loop successor: {s:#?}");
        }
    }

    let enclosing_loop_exits: Option<HashSet<NodeIndex>> = graph.loop_exits.get(&start).cloned();

    /// Classify one Condition/Switch arm:
    ///   - `target == start`: back-edge to a loop head; emit `Jump(DegenerateJumpIf)`.
    ///   - `loop_succ == Some(target)`: loop-head arm exits the loop. Emit `Jump(LoopBreak)`
    ///     for `insert_breaks` to convert to `Break`.
    ///   - `target` is an exit of an enclosing loop: emit `Jump(ArmOutsideSubtree)`. The
    ///     outer `structure_loop` will append `target` after its `Loop` form, and
    ///     `insert_breaks` will rewrite this Jump to a `Break`. We must not embed even if
    ///     `target` is singly entered — that would bury the loop exit inside the body.
    ///   - `target ∈ ichildren` and `target` is singly-entered (the only CFG predecessor
    ///     outside its own dom subtree is the edge from `start`): embed the structured form
    ///     as the arm body. Back-edges from inside `target`'s subtree don't count — a loop
    ///     head is entered once from outside, even though its latch loops back in.
    ///   - Otherwise: emit `Jump(ArmOutsideSubtree)`. If `target` is a join point in our
    ///     scope, the owned-children hoist below places it as a sibling and elides this
    ///     Jump. If `target` is owned by an ancestor scope, the Jump survives for that
    ///     scope's hoist or `insert_breaks`.
    fn arm_for(
        target: NodeIndex,
        start: NodeIndex,
        loop_succ: Option<NodeIndex>,
        loop_exits: Option<&HashSet<NodeIndex>>,
        cfg: &petgraph::graph::DiGraph<(), ()>,
        dom_tree: &dom_tree::DominatorTree,
        ichildren: &HashSet<NodeIndex>,
        structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    ) -> D::Structured {
        if target == start {
            D::Structured::Jump(GotoSource::DegenerateJumpIf, target)
        } else if Some(target) == loop_succ {
            D::Structured::Jump(GotoSource::LoopBreak, target)
        } else if loop_exits.is_some_and(|e| e.contains(&target)) {
            D::Structured::Jump(GotoSource::ArmOutsideSubtree, target)
        } else if ichildren.contains(&target) && is_singly_entered(target, cfg, dom_tree) {
            structured_blocks.remove(&target).unwrap()
        } else {
            D::Structured::Jump(GotoSource::ArmOutsideSubtree, target)
        }
    }

    // Use the same predicate as `arm_for` to record absorbed arms for back-edge ownership
    // transfer.
    let is_absorbed = |target: NodeIndex| -> bool {
        target != start
            && Some(target) != loop_succ
            && !enclosing_loop_exits
                .as_ref()
                .is_some_and(|e| e.contains(&target))
            && ichildren.contains(&target)
            && is_singly_entered(target, &graph.cfg, &graph.dom_tree)
    };

    let structured = match input.remove(&start).unwrap() {
        D::Input::Condition(_lbl, code, conseq, alt) => {
            let conseq_arm = arm_for(
                conseq,
                start,
                loop_succ,
                enclosing_loop_exits.as_ref(),
                &graph.cfg,
                &graph.dom_tree,
                &ichildren,
                structured_blocks,
            );
            let alt_arm = arm_for(
                alt,
                start,
                loop_succ,
                enclosing_loop_exits.as_ref(),
                &graph.cfg,
                &graph.dom_tree,
                &ichildren,
                structured_blocks,
            );

            // Transfer back-edge ownership for absorbed arms (only the ichildren-embed branch
            // of `arm_for` absorbs).
            let mut absorbed = vec![];
            if is_absorbed(conseq) {
                absorbed.push(conseq);
            }
            if is_absorbed(alt) {
                absorbed.push(alt);
            }
            if !absorbed.is_empty() {
                graph.update_latch_branch_nodes(start, absorbed);
            }

            graph.mark_emitted(code);
            D::Structured::IfElse(code, Box::new(conseq_arm), Box::new(Some(alt_arm)))
        }
        D::Input::Variants(_lbl, code, enum_, items) => {
            let latches = items
                .iter()
                .map(|(_v, item)| item)
                .filter(|item| Some(**item) != loop_succ)
                .cloned()
                .collect::<Vec<_>>();
            graph.update_latch_branch_nodes(start, latches);

            // Variant arms can share a target (e.g. several variants branching to the same
            // fall-through label). `arm_for`'s `ichildren`-embed path calls
            // `structured_blocks.remove(&target).unwrap()`, which would panic on the second
            // occurrence. With the in-degree-1 absorb predicate, a shared target also has
            // in-degree > 1, so `arm_for` naturally emits `Jump(AOS)` for each. We keep the
            // explicit count guard as a defensive belt-and-suspenders: even if some future
            // refactor weakens the predicate, the panic stays at bay.
            let mut counts: HashMap<NodeIndex, usize> = HashMap::new();
            for (_, item) in &items {
                *counts.entry(*item).or_insert(0) += 1;
            }
            let arms = items
                .into_iter()
                .map(|(v, item)| {
                    let body = if counts.get(&item).copied().unwrap_or(0) > 1 {
                        D::Structured::Jump(GotoSource::ArmOutsideSubtree, item)
                    } else {
                        arm_for(
                            item,
                            start,
                            loop_succ,
                            enclosing_loop_exits.as_ref(),
                            &graph.cfg,
                            &graph.dom_tree,
                            &ichildren,
                            structured_blocks,
                        )
                    };
                    (v, body)
                })
                .collect();
            graph.mark_emitted(code);
            // Maybe we could reconstruct matches from the arms? It would require a lot more —
            // and more painful — analysis.
            D::Structured::Switch(code, enum_, arms)
        }
        D::Input::Code(..) => unreachable!("Code shortcut at top of structure_acyclic_region"),
    };

    // Hoist orphan dom-tree children. After arm processing, any `ichildren` of `start` that
    // weren't absorbed as arms and aren't an enclosing loop's exit remain in
    // `structured_blocks`. They're owned by us — every CFG path to them goes through
    // `start` — so they semantically belong in our sequence. Append them, then walk the
    // tail positions of everything emitted so far to drop now-redundant `Jump`s to their
    // entry labels.
    //
    // Skip at the loop-head call: the loop's successor stays in `structured_blocks` so
    // `structure_loop` can append it after the `Loop` form, and body-side ichildren are
    // placed by the body-assembly logic. We also skip orphans that are succ_nodes of an
    // enclosing loop — `structure_loop` for that outer loop will append them after its
    // `Loop`, so we mustn't eat them at this inner level.
    //
    // TODO: alternate orphans. The current logic places every orphan as a sibling in
    // `hoist_order` (CFG-topo over the orphan-induced subgraph) and elides tail Jumps to
    // each. This is correct when the orphans form a chain (orphan A reaches orphan B
    // through the CFG, so A goes first and falls through to B), but it is *wrong* when
    // two orphans P and Q are mutually exclusive — reached on disjoint paths through
    // `start`'s arms (e.g. `start: A|B`; each of A/B further branches, and one branch
    // leads to P, the other to Q, on both sides). Both have `idom = start`, so both are
    // orphans; the CFG never visits both in a single execution, but sibling-hoist places
    // them as `IfElse(...); P_body; Q_body`, which falls through from P into Q. This is
    // masked when P's body always terminates (return/abort/break/continue), which covers
    // many corpus cases (shared cleanup blocks), but is unsound in general. The principled
    // fix is to wrap the IfElse in nested labeled blocks (`'go_q: { ... ; P_body; break
    // 'outer }; Q_body`) and rewrite the offending arm-Jumps to `break` of the appropriate
    // label — Move's `'label: { body }` form gives us exactly this control flow. That
    // requires an `Exp::LabeledBlock` first-class AST node plus detection of alternate
    // orphans here; tracked as a follow-up.
    let mut exp = vec![structured];
    if loop_succ.is_none() {
        let mut orphans: Vec<NodeIndex> = ichildren
            .iter()
            .copied()
            .filter(|c| structured_blocks.contains_key(c))
            .filter(|c| {
                enclosing_loop_exits
                    .as_ref()
                    .map(|exits| !exits.contains(c))
                    .unwrap_or(true)
            })
            .collect();
        // `ichildren` is a `HashSet`; sort for deterministic hoist order. `hoist_order`
        // then runs a topological sort over this CFG-stable input.
        orphans.sort_by_key(|n| n.index());
        for orphan in hoist_order(graph, start, &orphans) {
            let body = structured_blocks.remove(&orphan).unwrap();
            let target = entry_label(&body);
            if let Some(target) = target {
                // Walk every prior item's tails, dropping `Jump(_, target)` — these are
                // arm-tail branches that now fall through to the literal next instruction.
                for prior in exp.iter_mut() {
                    elide_tail_jump_to(prior, target);
                }
            }
            exp.push(body);
        }
    }
    D::Structured::Seq(exp)
}

/// Order orphan ichildren of `start` by CFG-reachability: each orphan should appear in the
/// Seq after the orphan(s) whose subtrees branch to it. Topological sort over the subgraph
/// induced by `orphans`, breaking cycles by index. In practice the orphan set is tiny
/// (typically just one — the post-dom of the IfElse) and any ordering works.
fn hoist_order(graph: &Graph, _start: NodeIndex, orphans: &[NodeIndex]) -> Vec<NodeIndex> {
    let cfg = &graph.cfg;
    let set: HashSet<NodeIndex> = orphans.iter().copied().collect();
    // `BTreeMap` for deterministic initial iteration when seeding the ready queue.
    let mut in_deg: BTreeMap<NodeIndex, usize> = orphans.iter().map(|&n| (n, 0)).collect();
    for &n in orphans {
        for succ in cfg.neighbors(n) {
            if set.contains(&succ) && succ != n {
                *in_deg.entry(succ).or_insert(0) += 1;
            }
        }
    }
    let mut ready: Vec<NodeIndex> = in_deg
        .iter()
        .filter_map(|(&n, &d)| (d == 0).then_some(n))
        .collect();
    ready.sort_by_key(|n| n.index());
    let mut out = Vec::new();
    while let Some(n) = ready.pop() {
        out.push(n);
        for succ in cfg.neighbors(n) {
            if let Some(d) = in_deg.get_mut(&succ) {
                *d -= 1;
                if *d == 0 {
                    ready.push(succ);
                    ready.sort_by_key(|n: &NodeIndex| n.index());
                }
            }
        }
    }
    // Anything left is on a cycle; append by index for determinism.
    let mut remaining: Vec<NodeIndex> = orphans
        .iter()
        .copied()
        .filter(|n| !out.contains(n))
        .collect();
    remaining.sort_by_key(|n| n.index());
    out.extend(remaining);
    out
}

fn structure_latch_node(
    config: &config::Config,
    graph: &mut Graph,
    node_ndx: NodeIndex,
    node: D::Input,
) -> D::Structured {
    if config.debug_print.structuring {
        println!("structuring latch node {node_ndx:#?}");
    }
    assert!(graph.back_edges.contains_key(&node_ndx));
    match node {
        D::Input::Condition(_, code, conseq, alt) => {
            graph.mark_emitted(code);
            D::Structured::JumpIf(GotoSource::LatchTest, code, conseq, alt)
        }
        D::Input::Code(_, code, next) => {
            graph.mark_emitted(code);
            D::Structured::Seq(vec![
                D::Structured::Block(code),
                D::Structured::Jump(GotoSource::LatchCode, next.unwrap()),
            ])
        }
        D::Input::Variants(_, _, _, _) => unreachable!(),
    }
}

fn structure_code_node(
    config: &config::Config,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    node_ndx: NodeIndex,
    node: D::Input,
) -> D::Structured {
    if config.debug_print.structuring {
        println!("structuring code node: {node:#?}");
    }
    match node {
        D::Input::Code(_, code, Some(next)) if next == node_ndx => {
            graph.mark_emitted(code);
            D::Structured::Seq(vec![
                D::Structured::Block(code),
                D::Structured::Jump(GotoSource::SelfLoop, next),
            ])
        }
        D::Input::Code(_, code, next) => {
            // Fuse `next` only if it's our exclusive dom-tree child — i.e. it's in
            // `ichildren` and singly entered (no other path from outside its own subtree
            // reaches it). For Code nodes specifically, `ichildren.contains(&next)` already
            // implies single-entry (Code has only one CFG successor, so any other
            // predecessor of `next` would prevent `next ∈ ichildren`); we spell the
            // `is_singly_entered` check explicitly so the principle reads identically to
            // `arm_for`'s: fold the target into our region iff control enters it
            // exclusively through this edge. The `enclosing_loop_exits` guard prevents
            // burying a loop-exit body inside an inner Code block; `structure_loop` will
            // append it after its `Loop`.
            let ichildren: HashSet<NodeIndex> =
                graph.dom_tree.get(node_ndx).immediate_children().collect();
            let enclosing_loop_exits: Option<HashSet<NodeIndex>> =
                graph.loop_exits.get(&node_ndx).cloned();
            graph.mark_emitted(code);
            let mut seq = vec![D::Structured::Block(code)];
            if let Some(next) = next {
                let fuse = ichildren.contains(&next)
                    && structured_blocks.contains_key(&next)
                    && is_singly_entered(next, &graph.cfg, &graph.dom_tree)
                    && !enclosing_loop_exits
                        .as_ref()
                        .is_some_and(|e| e.contains(&next));
                if fuse {
                    let successor = structured_blocks.remove(&next).unwrap();
                    graph.update_latch_nodes(node_ndx, next);
                    seq.push(successor);
                } else {
                    // `next` is not exclusively ours — either it's reached from other
                    // paths or it's owned by an enclosing structure. Emit an explicit
                    // `Jump(CodeBranch)` so the owned-children hoist or `insert_breaks`
                    // can see and rewrite it. Without this, the branch lives only in the
                    // bytecode terminator and is invisible to elision.
                    seq.push(D::Structured::Jump(GotoSource::CodeBranch, next));
                }
            }
            // Owned-children hoist: any `ichildren` of this Code block that we didn't embed
            // as `next` and that the enclosing-loop logic doesn't claim is a node every CFG
            // path through us reaches. Place them in dom-tree-order siblings of our Block,
            // and walk prior items' tails to elide `Jump`s targeting them.
            //
            // This is the same mechanism `structure_acyclic_region` runs after its
            // IfElse/Switch construction; without it, function bodies whose dom tree looks
            // like `entry -> {first_test, label_X, second_test, label_Y, ...}` lose every
            // sibling after `next` to orphan-in-structured_blocks, and the surviving Jumps
            // to those siblings stay as gotos.
            let mut orphans: Vec<NodeIndex> = ichildren
                .iter()
                .copied()
                .filter(|c| Some(*c) != next)
                .filter(|c| structured_blocks.contains_key(c))
                .filter(|c| {
                    enclosing_loop_exits
                        .as_ref()
                        .map(|exits| !exits.contains(c))
                        .unwrap_or(true)
                })
                .collect();
            orphans.sort_by_key(|n| n.index());
            for orphan in hoist_order(graph, node_ndx, &orphans) {
                let body = structured_blocks.remove(&orphan).unwrap();
                let target = entry_label(&body);
                if let Some(target) = target {
                    for prior in seq.iter_mut() {
                        elide_tail_jump_to(prior, target);
                    }
                }
                seq.push(body);
            }
            let mut result = D::Structured::Seq(seq);
            flatten_sequence(&mut result);
            result
        }
        D::Input::Condition(..) | D::Input::Variants(..) => unreachable!(),
    }
}

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

/// Final normalization pass over the structured output. Recursively drops empty `Seq`s,
/// splices non-empty nested `Seq`s into their parent, and collapses `Some(Seq([]))` alts
/// to `None`. Called once at the end of `structure()`; the per-scope code paths leave
/// stray empties from tail-elision and from `insert_breaks`'s in-loop-Jump replacement,
/// and this is the single place that normalizes them away.
fn flatten_sequence(s: &mut D::Structured) {
    use D::Structured as DS;
    match s {
        DS::Seq(items) => {
            for item in items.iter_mut() {
                flatten_sequence(item);
            }
            let mut flat = Vec::with_capacity(items.len());
            for item in items.drain(..) {
                match item {
                    DS::Seq(inner) if inner.is_empty() => {}
                    DS::Seq(inner) => flat.extend(inner),
                    other => flat.push(other),
                }
            }
            *items = flat;
        }
        DS::IfElse(_, conseq, alt) => {
            flatten_sequence(conseq);
            if let Some(alt_inner) = alt.as_mut().as_mut() {
                flatten_sequence(alt_inner);
            }
            // `arm_for` always returns *some* body, so an alt that elided away ends up as
            // an empty Seq; later refinements/printers treat `IfElse(_, _, None)` as the
            // canonical "no-else" shape.
            if matches!(alt.as_ref().as_ref(), Some(DS::Seq(items)) if items.is_empty()) {
                **alt = None;
            }
        }
        DS::Switch(_, _, cases) => {
            for (_, body) in cases.iter_mut() {
                flatten_sequence(body);
            }
        }
        DS::Loop(_, body) => flatten_sequence(body),
        DS::Block(_) | DS::Break(_) | DS::Continue(_) | DS::Jump(_, _) | DS::JumpIf(_, _, _, _) => {
        }
    }
}
