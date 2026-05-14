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
    structure_acyclic(
        config,
        graph,
        structured_blocks,
        loop_head,
        input,
        /*inside_loop*/ true,
    );
    let (loop_nodes, succ_nodes) = graph.find_loop_nodes(loop_head);
    if config.debug_print.structuring {
        println!("  loop nodes: {loop_nodes:#?}");
        println!("  successor nodes: {succ_nodes:#?}");
    }

    let mut loop_nodes_iter = loop_nodes.clone().into_iter().collect::<Vec<_>>();
    loop_nodes_iter.sort();

    let mut loop_body = vec![];

    let succ_node = succ_nodes.into_iter().next();
    for node in loop_nodes_iter.into_iter().rev() {
        let Some(node) = structured_blocks.remove(&node) else {
            continue;
        };
        let result = insert_breaks(&loop_nodes, loop_head, succ_node, node);
        loop_body.push(result);
    }

    let loop_body = loop_body.into_iter().rev().collect::<Vec<_>>();
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
) {
    if graph.back_edges.contains_key(&node) {
        let result = structure_latch_node(config, graph, node, input.remove(&node).unwrap());
        structured_blocks.insert(node, result);
    } else {
        let result =
            structure_acyclic_region(config, graph, structured_blocks, input, node, inside_loop);
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

    // True when every predecessor of `post_dominator` lies in start's dominator subtree.
    // Post-dom is then reachable only through us, so no outer scope will emit it; arm-jumps
    // targeting it become redundant and can be elided in favor of inline sequencing.
    let we_own_post_dom = if post_dominator == graph.return_ {
        false
    } else {
        let subtree: HashSet<NodeIndex> = dom_node
            .all_children()
            .chain(std::iter::once(start))
            .collect();
        graph
            .cfg
            .neighbors_directed(post_dominator, petgraph::Direction::Incoming)
            .all(|pred| subtree.contains(&pred))
    };
    // True when start and post_dominator share the same set of enclosing loops. Inlining
    // post_dom consumes it from `structured_blocks`; if it belongs to an outer scope (e.g.,
    // a post-loop continuation), the consuming structurer would drag it inside the wrong
    // region.
    let same_loop_scope = graph.loop_scope_of(start) == graph.loop_scope_of(post_dominator);
    let emit_post_dom_in_seq = post_dominator != graph.return_
        && we_own_post_dom
        && same_loop_scope
        && !graph.back_edges.contains_key(&start)
        && !inside_loop;

    if config.debug_print.dominators {
        println!("  we own post-dominator: {we_own_post_dom}");
        println!("  emit post-dominator in sequence: {emit_post_dom_in_seq}");
    }

    /// Classify one Condition arm and produce its structured form (or `None` to omit it):
    ///   - `target == start`: back-edge to a loop head; emit `Jump(DegenerateJumpIf)`.
    ///   - `target == post_dom`, owned and inline-sequenced: `None` (fall through).
    ///   - `target == post_dom`, not owned/sequenced: `Jump(pd_src)` for `insert_breaks`
    ///     to rewrite. `pd_src` distinguishes which arm produced it.
    ///   - `target` in start's dominator subtree: embed its already-structured form.
    ///   - Otherwise: `Jump(ArmOutsideSubtree)`. Target is owned by an enclosing structure;
    ///     the enclosing `insert_breaks` rewrites it.
    fn arm_for(
        target: NodeIndex,
        start: NodeIndex,
        post_dominator: NodeIndex,
        ichildren: &HashSet<NodeIndex>,
        structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
        emit_post_dom_in_seq: bool,
        pd_src: GotoSource,
    ) -> Option<D::Structured> {
        if target == start {
            Some(D::Structured::Jump(GotoSource::DegenerateJumpIf, target))
        } else if target == post_dominator {
            if emit_post_dom_in_seq {
                None
            } else {
                Some(D::Structured::Jump(pd_src, target))
            }
        } else if ichildren.contains(&target) {
            Some(structured_blocks.remove(&target).unwrap())
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
                emit_post_dom_in_seq,
                GotoSource::ConseqEqPostDom,
            );
            let alt_arm = arm_for(
                alt,
                start,
                post_dominator,
                &ichildren,
                structured_blocks,
                emit_post_dom_in_seq,
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
                    if post_dominator == item {
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
                        (v, structured_blocks.remove(&item).unwrap())
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

    // TODO ensure end nodes are not back edges
    let start_dominates_post_dom = graph
        .dom_tree
        .get(start)
        .all_children()
        .any(|child| child == post_dominator);
    let emit_post_dom_in_seq = ichildren.contains(&post_dominator)
        && !graph.back_edges.contains_key(&start)
        && start_dominates_post_dom
        && !inside_loop;

    if config.debug_print.dominators {
        println!("  start dominates post-dominator: {start_dominates_post_dom}");
        println!("  emit post-dominator in sequence: {emit_post_dom_in_seq}");
    }

    if post_dominator != graph.return_ && emit_post_dom_in_seq {
        if config.debug_print.dominators {
            println!("  => emitting post-dominator");
        }
        exp.push(structured_blocks.remove(&post_dominator).unwrap());
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
