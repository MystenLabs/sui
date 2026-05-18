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

use petgraph::{graph::NodeIndex, visit::DfsPostOrder};

use std::collections::{BTreeMap, HashSet, VecDeque};

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
) -> D::Structured {
    // Native functions have empty basic blocks - return early to avoid panicking in Graph::new
    if input.is_empty() {
        return D::Structured::Seq(vec![]);
    }

    let mut graph = Graph::new(config, &input, entry_node);

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

    structured_blocks.remove(&entry_node).unwrap()
}

// -------------------------------------------------------------------------------------------------
// Threaded fall-through info: `next_in_outer`
// -------------------------------------------------------------------------------------------------
// Every call into `structure_acyclic` carries a `next_in_outer: Option<NodeIndex>` — the CFG
// node that will be placed immediately after this node's structured form in its containing
// Seq. The structurer uses it to make a single decision at emission time:
//   - In `arm_for`, the effective adjacent-next-sibling is `post_dom` when we inline it,
//     else `next_in_outer`. An arm whose target equals that sibling becomes `None` (a
//     fall-through) instead of a `Jump`.
//
// Pre-structured children come out of `structured_blocks` having been built without this
// context (post-order structures children before their consumer). When we consume one, we
// walk its tail positions with `elide_tail_jump_to` against the same adjacent-next target —
// the same decision the child would have made if it had known.
//
// Callers compute the value they pass down:
//   - `structure_nodes`: `None` — no surrounding Seq.
//   - `structure_loop` head: the loop's successor node — what follows the `Loop` form.
//   - `structure_acyclic_region`: when inlining post_dom, the post_dom block's tails see
//     our own `next_in_outer`.
//
// Loop-body RPO adjacency (body[i]'s next is body[i+1]) is the one case the threading
// doesn't reach: body items are structured by `structure_nodes` before the body is laid out,
// so they get `None`. `structure_loop`'s body assembly walks pairwise with the same helper.

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
/// item, both `IfElse` arms, and every `Switch` arm. Doesn't descend into `Loop` bodies -
/// they don't fall through.
fn elide_tail_jump_to(s: &mut D::Structured, target: NodeIndex) {
    use D::Structured as DS;
    match s {
        DS::Jump(_, label) if *label == target => {
            *s = DS::Seq(vec![]);
        }
        DS::Seq(items) => {
            if let Some(last) = items.last_mut() {
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
                /*inside_loop*/ false,
                /*next_in_outer*/ None,
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
    let succ_node = succ_nodes.iter().copied().next();
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
        /*inside_loop*/ true,
        /*next_in_outer*/ succ_node,
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
    inside_loop: bool,
    next_in_outer: Option<NodeIndex>,
) {
    if graph.back_edges.contains_key(&node) {
        let result = structure_latch_node(config, graph, node, input.remove(&node).unwrap());
        structured_blocks.insert(node, result);
    } else {
        let result = structure_acyclic_region(
            config,
            graph,
            structured_blocks,
            input,
            node,
            inside_loop,
            next_in_outer,
        );
        structured_blocks.insert(node, result);
    }
}

fn structure_acyclic_region(
    config: &config::Config,
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    input: &mut BTreeMap<D::Label, D::Input>,
    start: NodeIndex,
    inside_loop: bool,
    next_in_outer: Option<NodeIndex>,
) -> D::Structured {
    // Code has no arms and no post-dominator role; delegate before the dom-tree work.
    if matches!(&input[&start], D::Input::Code(..)) {
        let code_node = input.remove(&start).unwrap();
        return structure_code_node(config, graph, structured_blocks, start, code_node);
    }

    let dom_node = graph.dom_tree.get(start);
    let ichildren = dom_node.immediate_children().collect::<HashSet<_>>();
    let post_dominator = graph.post_dominators.immediate_dominator(start).unwrap();

    if config.debug_print.structuring {
        println!("structuring acyclic region at node {start:#?}");
        println!("  blocks: {structured_blocks:#?}");
        println!("  immediate children: {ichildren:#?}");
        println!("  post dominator: {post_dominator:#?}");
    }
    if post_dominator != graph.return_ {
        assert!(
            structured_blocks.contains_key(&post_dominator)
            // Technically, it should be "my_loop_head" but we don't have that info here
            || graph.loop_heads.contains(&post_dominator)
        );
    }

    // Arm processing below absorbs child structured blocks and transfers their back-edges
    // to `start` via `update_latch_branch_nodes`. The inline decision has to reflect that
    // post-absorption state — predict it now so `arm_for` and the actual inline agree.
    let absorbed_children: Vec<NodeIndex> = match &input[&start] {
        D::Input::Condition(_, _, c, a) => [*c, *a]
            .into_iter()
            .filter(|t| *t != start && *t != post_dominator && ichildren.contains(t))
            .collect(),
        D::Input::Variants(_, _, _, items) => items
            .iter()
            .map(|(_, t)| *t)
            .filter(|t| *t != post_dominator)
            .collect(),
        D::Input::Code(..) => vec![],
    };
    let start_has_back_edges_post_absorption = graph.back_edges.contains_key(&start)
        || absorbed_children
            .iter()
            .any(|t| graph.back_edges.contains_key(t));

    // True iff this structurer will sequence `post_dominator` immediately after the
    // IfElse/Switch in its outer Seq. The predicate has to match what actually happens
    // below, because `arm_for` uses it to decide between emitting a `Jump` and an empty
    // fall-through arm.
    //
    // `ichildren.contains(&post_dominator)` is the load-bearing membership check: an
    // immediate dom-tree child of `start` is by construction both (a) dominated by `start`
    // and (b) in the same enclosing-loop scope as `start` (the dom tree doesn't cross loop
    // boundaries for immediate children), so we don't need separate predicates for those.
    let emit_post_dom_in_seq = post_dominator != graph.return_
        && ichildren.contains(&post_dominator)
        && !start_has_back_edges_post_absorption
        && !inside_loop;

    if config.debug_print.dominators {
        println!("  emit post-dominator in sequence: {emit_post_dom_in_seq}");
    }

    // The CFG node that will be sequenced immediately after this IfElse/Switch in its
    // enclosing Seq. If we inline `post_dominator`, that's the inlined block; otherwise
    // our caller's `next_in_outer` is what they'll place next.
    //
    // For the loop-head call (`inside_loop`), `next_in_outer` is the loop's successor —
    // which arms reach as a *break* through `insert_breaks`, not as a fall-through. We
    // must leave those arm Jumps in place for `insert_breaks` to reclassify.
    let arms_next_sibling = if inside_loop {
        None
    } else if emit_post_dom_in_seq {
        Some(post_dominator)
    } else {
        next_in_outer
    };

    /// Classify one Condition arm and produce its structured form (or `None` to omit it):
    ///   - `target == start`: back-edge to a loop head; emit `Jump(DegenerateJumpIf)`.
    ///   - `target == arms_next_sibling`: the literal next instruction after our IfElse;
    ///     omit (fall through). Subsumes the inline-post_dom case via the computation of
    ///     `arms_next_sibling` above.
    ///   - `target == post_dom` (but not adjacent): `Jump(pd_src)`. `pd_src` distinguishes
    ///     which arm produced it.
    ///   - `target` in start's dominator subtree: embed its already-structured form, with
    ///     tail-`Jump`s to `arms_next_sibling` elided — the child was structured before us
    ///     and so didn't know what would be sequenced after it.
    ///   - Otherwise: `Jump(ArmOutsideSubtree)`. Target is owned by an enclosing structure;
    ///     the enclosing `insert_breaks` rewrites it.
    fn arm_for(
        target: NodeIndex,
        start: NodeIndex,
        post_dominator: NodeIndex,
        ichildren: &HashSet<NodeIndex>,
        structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
        arms_next_sibling: Option<NodeIndex>,
        pd_src: GotoSource,
    ) -> Option<D::Structured> {
        if target == start {
            Some(D::Structured::Jump(GotoSource::DegenerateJumpIf, target))
        } else if Some(target) == arms_next_sibling {
            None
        } else if target == post_dominator {
            Some(D::Structured::Jump(pd_src, target))
        } else if ichildren.contains(&target) {
            let mut arm = structured_blocks
                .remove(&target)
                .expect("ichildren member must be structured by post-order traversal");
            if let Some(next) = arms_next_sibling {
                elide_tail_jump_to(&mut arm, next);
            }
            Some(arm)
        } else {
            Some(D::Structured::Jump(GotoSource::ArmOutsideSubtree, target))
        }
    }

    let structured = match input.remove(&start).unwrap() {
        D::Input::Condition(_lbl, code, conseq, alt) => {
            // No `ichildren.contains(&conseq)` assertion: `arm_for` handles every case
            // (in-subtree, post-dom, self-edge, or outside-subtree).
            let conseq_arm = arm_for(
                conseq,
                start,
                post_dominator,
                &ichildren,
                structured_blocks,
                arms_next_sibling,
                GotoSource::ConseqEqPostDom,
            );
            let alt_arm = arm_for(
                alt,
                start,
                post_dominator,
                &ichildren,
                structured_blocks,
                arms_next_sibling,
                GotoSource::AltEqPostDom,
            );

            // Transfer back-edge ownership for absorbed arms (arms whose structured body we
            // embedded; only the `ichildren` branch of `arm_for` absorbs). Arms that emit a
            // Jump marker keep their own bookkeeping.
            let mut absorbed = vec![];
            if conseq != start && ichildren.contains(&conseq) && conseq != post_dominator {
                absorbed.push(conseq);
            }
            if alt != start && ichildren.contains(&alt) && alt != post_dominator {
                absorbed.push(alt);
            }
            if !absorbed.is_empty() {
                graph.update_latch_branch_nodes(start, absorbed);
            }

            match (conseq_arm, alt_arm) {
                // Both arms collapsed to the inline post-dom. Conditional reduces to
                // evaluating `code` for effect; the post-dom block is sequenced below.
                (None, None) => D::Structured::Block(code),
                // Empty-then with non-empty else. A later `Exp::IfElse` refinement can
                // negate the condition and flip into a non-empty `then`.
                (None, Some(body)) => D::Structured::IfElse(
                    code,
                    Box::new(D::Structured::Seq(vec![])),
                    Box::new(Some(body)),
                ),
                (Some(body), None) => D::Structured::IfElse(code, Box::new(body), Box::new(None)),
                (Some(c), Some(a)) => D::Structured::IfElse(code, Box::new(c), Box::new(Some(a))),
            }
        }
        D::Input::Variants(_lbl, code, enum_, items) => {
            let latches = items
                .iter()
                .map(|(_v, item)| item)
                .filter(|item| item != &&post_dominator)
                .cloned()
                .collect::<Vec<_>>();
            graph.update_latch_branch_nodes(start, latches);

            let arms = items
                .into_iter()
                .map(|(v, item)| {
                    if item == post_dominator || Some(item) == arms_next_sibling {
                        (v, D::Structured::Seq(vec![]))
                    } else {
                        assert!(
                            graph
                                .cfg
                                .neighbors_directed(item, petgraph::Direction::Incoming)
                                .count()
                                == 1,
                            "Structured arms must have exactly one predecessor, found {:#?} for {:#?}",
                            graph
                                .cfg
                                .neighbors_directed(item, petgraph::Direction::Incoming)
                                .collect::<Vec<_>>(),
                            item
                        );
                        let mut arm = structured_blocks
                            .remove(&item)
                            .expect("Switch arm must be structured by post-order traversal");
                        if let Some(next) = arms_next_sibling {
                            elide_tail_jump_to(&mut arm, next);
                        }
                        (v, arm)
                    }
                })
                .collect();
            // Maybe we could reconstruct matches from the arms? It would require a lot more -- and
            // more painful -- analysis.
            D::Structured::Switch(code, enum_, arms)
        }
        D::Input::Code(..) => unreachable!("Code shortcut at top of structure_acyclic_region"),
    };
    let mut exp = vec![structured];

    if emit_post_dom_in_seq {
        if config.debug_print.dominators {
            println!("  => emitting post-dominator");
        }
        let mut pd_block = structured_blocks
            .remove(&post_dominator)
            .expect("post-dom in ichildren must be structured by post-order traversal");
        // The inlined post-dom is sequenced next; whatever follows it in our Seq is what
        // our caller will place after us (their `next_in_outer`). Walk pre-built tails.
        if let Some(next) = next_in_outer {
            elide_tail_jump_to(&mut pd_block, next);
        }
        exp.push(pd_block);
    }
    flatten_sequence(D::Structured::Seq(exp))
}

fn structure_latch_node(
    config: &config::Config,
    graph: &Graph,
    node_ndx: NodeIndex,
    node: D::Input,
) -> D::Structured {
    if config.debug_print.structuring {
        println!("structuring latch node {node_ndx:#?}");
    }
    assert!(graph.back_edges.contains_key(&node_ndx));
    match node {
        D::Input::Condition(_, code, conseq, alt) => {
            D::Structured::JumpIf(GotoSource::LatchTest, code, conseq, alt)
        }
        D::Input::Code(_, code, next) => D::Structured::Seq(vec![
            D::Structured::Block(code),
            D::Structured::Jump(GotoSource::LatchCode, next.unwrap()),
        ]),
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
        D::Input::Code(_, code, Some(next)) if next == node_ndx => D::Structured::Seq(vec![
            D::Structured::Block(code),
            D::Structured::Jump(GotoSource::SelfLoop, next),
        ]),
        D::Input::Code(_, code, next) => {
            let mut seq = vec![D::Structured::Block(code)];
            if let Some(next) = next
                && graph
                    .dom_tree
                    .get(node_ndx)
                    .immediate_children()
                    .any(|node| node == next)
            {
                let successor = structured_blocks.remove(&next).unwrap();
                graph.update_latch_nodes(node_ndx, next);
                seq.push(successor);
            }
            flatten_sequence(D::Structured::Seq(seq))
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

fn flatten_sequence(seq: D::Structured) -> D::Structured {
    use D::Structured as DS;

    let mut result = vec![];
    let mut queue = VecDeque::from([seq]);

    while let Some(entry) = queue.pop_front() {
        match entry {
            DS::Seq(structureds) => {
                for entry in structureds.into_iter().rev() {
                    queue.push_front(entry);
                }
            }
            DS::Block(_)
            | DS::Break(_)
            | DS::Loop(_, _)
            | DS::IfElse(_, _, _)
            | DS::Switch(_, _, _)
            | DS::Continue(_)
            | DS::Jump(_, _)
            | DS::JumpIf(_, _, _, _) => result.push(entry),
        }
    }

    DS::Seq(result)
}
