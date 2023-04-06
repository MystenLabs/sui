// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module implements control flow checks.
//!
//! For bytecode versions 6 and up, the following properties are ensured:
//! - The CFG is not empty and the last block ends in an unconditional jump, so it's not possible to
//!   fall off the end of a function.
//! - The CFG is reducible (and optionally max loop depth is bounded), to limit the potential for
//!   pathologically long abstract interpretation runtimes (through poor choice of loop heads and
//!   back edges).
//!
//! For bytecode versions 5 and below, delegates to `control_flow_v5`.
use crate::{
    control_flow_v5,
    loop_summary::{LoopPartition, LoopSummary},
    meter::Meter,
    verifier::VerifierConfig,
};
use move_binary_format::{
    access::{ModuleAccess, ScriptAccess},
    binary_views::FunctionView,
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        CodeOffset, CodeUnit, CompiledScript, FunctionDefinition, FunctionDefinitionIndex,
    },
    CompiledModule,
};
use move_core_types::vm_status::StatusCode;
use std::collections::BTreeSet;

/// Perform control flow verification on the compiled function, returning its `FunctionView` if
/// verification was successful.
pub fn verify_function<'a>(
    verifier_config: &'a VerifierConfig,
    module: &'a CompiledModule,
    index: FunctionDefinitionIndex,
    function_definition: &'a FunctionDefinition,
    code: &'a CodeUnit,
    _meter: &mut impl Meter, // TODO: metering
) -> PartialVMResult<FunctionView<'a>> {
    let function_handle = module.function_handle_at(function_definition.function);

    if module.version() <= 5 {
        control_flow_v5::verify(verifier_config, Some(index), code)?;
        Ok(FunctionView::function(module, index, code, function_handle))
    } else {
        verify_fallthrough(Some(index), code)?;
        let function_view = FunctionView::function(module, index, code, function_handle);
        verify_reducibility(verifier_config, &function_view)?;
        Ok(function_view)
    }
}

/// Perform control flow verification on the compiled script, returning its `FunctionView` if
/// verification was successful.
pub fn verify_script<'a>(
    verifier_config: &'a VerifierConfig,
    script: &'a CompiledScript,
) -> PartialVMResult<FunctionView<'a>> {
    if script.version() <= 5 {
        control_flow_v5::verify(verifier_config, None, &script.code)?;
        Ok(FunctionView::script(script))
    } else {
        verify_fallthrough(None, &script.code)?;
        let function_view = FunctionView::script(script);
        verify_reducibility(verifier_config, &function_view)?;
        Ok(function_view)
    }
}

/// Check to make sure that the bytecode vector is non-empty and ends with a branching instruction.
fn verify_fallthrough(
    current_function_opt: Option<FunctionDefinitionIndex>,
    code: &CodeUnit,
) -> PartialVMResult<()> {
    let current_function = current_function_opt.unwrap_or(FunctionDefinitionIndex(0));
    match code.code.last() {
        None => Err(PartialVMError::new(StatusCode::EMPTY_CODE_UNIT)),
        Some(last) if !last.is_unconditional_branch() => {
            Err(PartialVMError::new(StatusCode::INVALID_FALL_THROUGH)
                .at_code_offset(current_function, (code.code.len() - 1) as CodeOffset))
        }
        Some(_) => Ok(()),
    }
}

/// Test that `function_view`'s control-flow graph is reducible using Tarjan's algorithm [1].
/// Optionally test loop depth bounded by `verifier_config.max_loop_depth`.
///
/// A CFG, `G`, with starting block `s` is reducible if and only if [2] any of the following
/// equivalent properties hold:
///
///  1. G has a unique set of back-edges `u -> v` where `v` dominates `u`, that corresponds to the
///     set of back-edges for any depth-first spanning tree of G.
///
///  2. Every loop in G contains a unique node `h` (the "head") which dominates all other nodes in
///     the loop.
///
///  3. G has a unique maximal (in terms of number of edges) acyclic sub-graph.
///
///  4. G can be reduced to a CFG containing just `s` through a sequence of the following two
///     operations:
///      a. Delete a cyclic edge `v -> v`
///      b. For an edge `e: u -> v` where `e` is the only incident edge to `v`, collapse `v` into `u`
///         by deleting `e` and `v` and replacing all `v -> w` edges with `u -> w` edges.
///
/// Reducibility means that a control-flow graph can be decomposed into a series of nested loops
/// (strongly connected subgraphs), which leads to more predictable abstract interpretation
/// performance.
///
/// ## References
///
///  1. Tarjan, R.  1974.  Testing Flow Graph Reducibility.
///  2. Hecht, M. S., Ullman J. D.  1974.  Characterizations of Reducible Flow Graphs.
fn verify_reducibility<'a>(
    verifier_config: &VerifierConfig,
    function_view: &'a FunctionView<'a>,
) -> PartialVMResult<()> {
    let current_function = function_view.index().unwrap_or(FunctionDefinitionIndex(0));
    let err = move |code: StatusCode, offset: CodeOffset| {
        Err(PartialVMError::new(code).at_code_offset(current_function, offset))
    };

    let summary = LoopSummary::new(function_view.cfg());
    let mut partition = LoopPartition::new(&summary);

    // Iterate through nodes in reverse pre-order so more deeply nested loops (which would appear
    // later in the pre-order) are processed first.
    for head in summary.preorder().rev() {
        // If a node has no back edges, it is not a loop head, so doesn't need to be processed.
        let back = summary.back_edges(head);
        if back.is_empty() {
            continue;
        }

        // Collect the rest of the nodes in `head`'s loop, in `body`.  Start with the nodes that
        // jump back to the head, and grow `body` by repeatedly following predecessor edges until
        // `head` is found again.

        let mut body = BTreeSet::new();
        for node in back {
            let node = partition.containing_loop(*node);

            if node != head {
                body.insert(node);
            }
        }

        let mut frontier: Vec<_> = body.iter().copied().collect();
        while let Some(node) = frontier.pop() {
            for pred in summary.pred_edges(node) {
                let pred = partition.containing_loop(*pred);

                // `pred` can eventually jump back to `head`, so is part of its body.  If it is not
                // a descendant of `head`, it implies that `head` does not dominate a node in its
                // loop, therefore the CFG is not reducible, according to Property 1 (see doc
                // comment).
                if !summary.is_descendant(/* ancestor */ head, /* descendant */ pred) {
                    return err(StatusCode::INVALID_LOOP_SPLIT, summary.block(pred));
                }

                let body_extended = pred != head && body.insert(pred);
                if body_extended {
                    frontier.push(pred);
                }
            }
        }

        // Collapse all the nodes in `body` into `head`, so it appears as one node when processing
        // outer loops (this performs a sequence of Operation 4(b), followed by a 4(a)).
        let depth = partition.collapse_loop(head, &body);
        if let Some(max_depth) = verifier_config.max_loop_depth {
            if depth as usize > max_depth {
                return err(StatusCode::LOOP_MAX_DEPTH_REACHED, summary.block(head));
            }
        }
    }

    Ok(())
}
