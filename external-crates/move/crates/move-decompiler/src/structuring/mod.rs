// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod ast;
pub(crate) mod dom_tree;
pub(crate) mod graph;
pub(crate) mod term_reconstruction;

use crate::structuring::{ast as D, graph::Graph};

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
    mut input: BTreeMap<D::Label, D::Input>,
    entry_node: D::Label,
) -> D::Structured {
    let mut graph = Graph::new(&input, entry_node);

    let mut structured_blocks: BTreeMap<D::Label, D::Structured> = BTreeMap::new();

    let mut post_order = DfsPostOrder::new(&graph.cfg, entry_node);

    while let Some(node) = post_order.next(&graph.cfg) {
        if graph.loop_heads.contains(&node) {
            structure_loop(&mut graph, &mut structured_blocks, node, &mut input);
        } else {
            structure_acyclic(&mut graph, &mut structured_blocks, node, &mut input)
        }
    }

    structured_blocks.remove(&entry_node).unwrap()
}

fn structure_loop(
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    loop_head: NodeIndex,
    input: &mut BTreeMap<D::Label, D::Input>,
) {
    println!("Structuring loop at node {loop_head:#?}");
    structure_acyclic(graph, structured_blocks, loop_head, input);
    let (loop_nodes, succ_nodes) = graph.find_loop_nodes(loop_head);

    let mut loop_nodes_iter = loop_nodes.clone().into_iter().collect::<Vec<_>>();
    loop_nodes_iter.sort();

    let mut loop_body = vec![];

    let succ_node = succ_nodes.into_iter().next();
    for node in loop_nodes_iter.into_iter().rev() {
        let Some(node) = structured_blocks.remove(&node) else {
            continue;
        };
        let result = insert_breaks(&loop_nodes, loop_head, succ_node, node, graph);
        loop_body.push(result);
    }

    let loop_body = loop_body.into_iter().rev().collect::<Vec<_>>();
    let seq = D::Structured::Seq(loop_body);
    graph.update_loop_info(loop_head);
    let mut result = D::Structured::Loop(Box::new(seq));
    if let Some(succ_node) = succ_node {
        if graph
            .dom_tree
            .get(loop_head)
            .all_children()
            .any(|child| child == succ_node)
        {
            result =
                D::Structured::Seq(vec![result, structured_blocks.remove(&succ_node).unwrap()]);
        }
    }
    structured_blocks.insert(loop_head, result);
}

fn insert_breaks(
    loop_nodes: &HashSet<NodeIndex>,
    loop_head: NodeIndex,
    succ_node: Option<NodeIndex>,
    node: D::Structured,
    graph: &Graph,
) -> D::Structured {
    use D::Structured as DS;

    enum LatchKind {
        Continue,
        Break,
        InLoop,
        OtherLoop,
        Latch,
    }

    fn find_latch_kind(
        loop_nodes: &HashSet<NodeIndex>,
        loop_head: NodeIndex,
        succ_node: Option<NodeIndex>,
        node_ndx: NodeIndex,
        graph: &Graph,
    ) -> LatchKind {
        if node_ndx == loop_head {
            LatchKind::Continue
        } else if Some(node_ndx) == succ_node {
            LatchKind::Break
        } else if graph.loop_heads.contains(&node_ndx) {
            LatchKind::OtherLoop
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
                .map(|node| insert_breaks(loop_nodes, loop_head, succ_node, node, graph))
                .collect::<Vec<_>>(),
        ),
        DS::Break => node,
        DS::IfElse(code, conseq, alt) => DS::IfElse(
            code,
            Box::new(insert_breaks(
                loop_nodes, loop_head, succ_node, *conseq, graph,
            )),
            Box::new(alt.map(|alt| insert_breaks(loop_nodes, loop_head, succ_node, alt, graph))),
        ),
        DS::Jump(next) => match find_latch_kind(loop_nodes, loop_head, succ_node, next, graph) {
            LatchKind::Continue => DS::Continue,
            LatchKind::Break => DS::Break,
            // TODO check if jump target is the next node
            LatchKind::InLoop => D::Structured::Seq(vec![]),
            LatchKind::OtherLoop => DS::Jump(next),
            LatchKind::Latch => node,
        },
        DS::JumpIf(code, next, other) => {
            match (
                find_latch_kind(loop_nodes, loop_head, succ_node, next, graph),
                find_latch_kind(loop_nodes, loop_head, succ_node, other, graph),
            ) {
                (LatchKind::Continue, LatchKind::Continue) => DS::Continue,
                (LatchKind::Break, LatchKind::Break) => DS::Break,
                (LatchKind::Continue, LatchKind::Break) => DS::Seq(vec![
                    DS::IfElse(code, Box::new(DS::Continue), Box::new(None)),
                    DS::Break,
                ]),
                (LatchKind::Continue, LatchKind::InLoop) => {
                    DS::IfElse(code, Box::new(DS::Continue), Box::new(None))
                }
                (LatchKind::Continue, LatchKind::Latch) => DS::IfElse(
                    code,
                    Box::new(DS::Continue),
                    Box::new(Some(DS::Jump(other))),
                ),
                (LatchKind::Break, LatchKind::Continue) => DS::Seq(vec![
                    DS::IfElse(code, Box::new(DS::Break), Box::new(None)),
                    DS::Continue,
                ]),
                (LatchKind::Break, LatchKind::InLoop) => {
                    DS::IfElse(code, Box::new(DS::Break), Box::new(None))
                }
                (LatchKind::Break, LatchKind::Latch) => {
                    DS::IfElse(code, Box::new(DS::Break), Box::new(Some(DS::Jump(other))))
                }
                (LatchKind::InLoop, LatchKind::Continue) => DS::IfElse(
                    code,
                    Box::new(DS::Seq(vec![])),
                    Box::new(Some(DS::Continue)),
                ),
                (LatchKind::InLoop, LatchKind::Break) => {
                    DS::IfElse(code, Box::new(DS::Seq(vec![])), Box::new(Some(DS::Break)))
                }
                (LatchKind::InLoop, LatchKind::InLoop) => unreachable!(),
                (LatchKind::InLoop, LatchKind::Latch) => DS::IfElse(
                    code,
                    Box::new(DS::Seq(vec![])),
                    Box::new(Some(DS::Jump(other))),
                ),
                (LatchKind::Latch, LatchKind::Continue) => {
                    DS::IfElse(code, Box::new(DS::Jump(next)), Box::new(Some(DS::Continue)))
                }
                (LatchKind::Latch, LatchKind::Break) => {
                    DS::IfElse(code, Box::new(DS::Jump(next)), Box::new(Some(DS::Break)))
                }
                (LatchKind::Latch, LatchKind::InLoop) => {
                    DS::IfElse(code, Box::new(DS::Jump(next)), Box::new(None))
                }
                (LatchKind::Latch, LatchKind::Latch) => DS::JumpIf(code, next, other),
                // TODO handle otherloop cases
                (LatchKind::Continue, LatchKind::OtherLoop) => todo!(),
                (LatchKind::Break, LatchKind::OtherLoop) => todo!(),
                (LatchKind::InLoop, LatchKind::OtherLoop) => todo!(),
                (LatchKind::OtherLoop, LatchKind::Continue) => todo!(),
                (LatchKind::OtherLoop, LatchKind::Break) => todo!(),
                (LatchKind::OtherLoop, LatchKind::InLoop) => todo!(),
                (LatchKind::OtherLoop, LatchKind::OtherLoop) => todo!(),
                (LatchKind::OtherLoop, LatchKind::Latch) => todo!(),
                (LatchKind::Latch, LatchKind::OtherLoop) => todo!(),
            }
        }
        DS::Continue => DS::Continue,
        DS::Loop(structured) => DS::Loop(Box::new(insert_breaks(
            loop_nodes,
            loop_head,
            succ_node,
            *structured,
            graph,
        ))),
        DS::Switch(code, enum_, structureds) => {
            let result = structureds
                .into_iter()
                .map(|(v, structured)| {
                    (
                        v,
                        insert_breaks(loop_nodes, loop_head, succ_node, structured, graph),
                    )
                })
                .collect::<Vec<_>>();
            DS::Switch(code, enum_, result)
        }
    }
}

fn structure_acyclic(
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    node: NodeIndex,
    input: &mut BTreeMap<D::Label, D::Input>,
) {
    let dom_node = graph.dom_tree.get(node);
    if graph.back_edges.contains_key(&node) {
        let result = structure_latch_node(graph, node, input.remove(&node).unwrap());
        structured_blocks.insert(node, result);
    } else if dom_node.all_children().count() > 0 {
        let result = structure_acyclic_region(graph, structured_blocks, input, node);
        structured_blocks.insert(node, result);
    } else {
        assert!(matches!(&input[&node], D::Input::Code(..)));
        let result =
            structure_code_node(graph, structured_blocks, node, input.remove(&node).unwrap());
        structured_blocks.insert(node, result);
    }
}

fn structure_acyclic_region(
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    input: &mut BTreeMap<D::Label, D::Input>,
    start: NodeIndex,
) -> D::Structured {
    let dom_node = graph.dom_tree.get(start);
    let ichildren = dom_node.immediate_children().collect::<HashSet<_>>();
    let post_dominator = graph.post_dominators.immediate_dominator(start).unwrap();

    println!("Structuring Node: {start:#?}");
    println!("Blocks: {structured_blocks:#?}");
    // println!("Post dominator: {post_dominator:#?}");
    // println!("Immediate children: {ichildren:#?}");
    if post_dominator != graph.return_ {
        assert!(
            structured_blocks.contains_key(&post_dominator)
            // Technically, it should be "my_loop_head" but we don't have that info here
            || graph.loop_heads.contains(&post_dominator)
        );
    }

    let structured = match input.remove(&start).unwrap() {
        D::Input::Condition(_, code, conseq, alt) if conseq == start || alt == start => {
            return D::Structured::JumpIf(code, conseq, alt);
        }
        D::Input::Condition(_lbl, code, conseq, alt) => {
            assert!(ichildren.contains(&conseq));

            let conseq_arm = if conseq != post_dominator {
                structured_blocks.remove(&conseq).unwrap()
            } else {
                D::Structured::Jump(conseq)
            };
            let alt = if ichildren.contains(&alt) && alt != post_dominator {
                graph.update_latch_branch_nodes(start, vec![conseq, alt]);
                let alt = structured_blocks.remove(&alt);
                assert!(alt.is_some());
                alt
            } else if alt == post_dominator {
                graph.update_latch_branch_nodes(start, vec![conseq]);
                Some(D::Structured::Jump(alt))
            } else {
                graph.update_latch_nodes(start, conseq);
                None
            };
            D::Structured::IfElse(code, Box::new(conseq_arm), Box::new(alt))
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
        code @ D::Input::Code(..) => {
            return structure_code_node(graph, structured_blocks, start, code);
        }
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
        && start_dominates_post_dom;

    // println!("Start dominates post-dominator: {start_dominates_post_dom}");
    // println!("Emit post dominator in sequence: {emit_post_dom_in_seq}");

    if post_dominator != graph.return_ && emit_post_dom_in_seq {
        // println!("Emitting post-dominator");
        exp.push(structured_blocks.remove(&post_dominator).unwrap());
    }
    flatten_sequence(D::Structured::Seq(exp))
}

fn structure_latch_node(graph: &Graph, node_ndx: NodeIndex, node: D::Input) -> D::Structured {
    println!("Structuring Latch Node: {node:#?}");
    assert!(graph.back_edges.contains_key(&node_ndx));
    match node {
        D::Input::Condition(_, code, conseq, alt) => D::Structured::JumpIf(code, conseq, alt),
        D::Input::Code(_, code, next) => D::Structured::Seq(vec![
            D::Structured::Block(code),
            D::Structured::Jump(next.unwrap()),
        ]),
        D::Input::Variants(_, _, _, _) => unreachable!(),
    }
}

fn structure_code_node(
    graph: &mut Graph,
    structured_blocks: &mut BTreeMap<NodeIndex, D::Structured>,
    node_ndx: NodeIndex,
    node: D::Input,
) -> D::Structured {
    println!("Structuring Code Node: {node:#?}");
    match node {
        D::Input::Code(_, code, Some(next)) if next == node_ndx => {
            D::Structured::Seq(vec![D::Structured::Block(code), D::Structured::Jump(next)])
        }
        D::Input::Code(_, code, next) => {
            let mut seq = vec![D::Structured::Block(code)];
            if let Some(next) = next {
                if graph
                    .dom_tree
                    .get(node_ndx)
                    .immediate_children()
                    .any(|node| node == next)
                {
                    let successor = structured_blocks.remove(&next).unwrap();
                    graph.update_latch_nodes(node_ndx, next);
                    seq.push(successor);
                }
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
            | DS::Break
            | DS::Loop(_)
            | DS::IfElse(_, _, _)
            | DS::Switch(_, _, _)
            | DS::Continue
            | DS::Jump(_)
            | DS::JumpIf(_, _, _) => result.push(entry),
        }
    }

    DS::Seq(result)
}
