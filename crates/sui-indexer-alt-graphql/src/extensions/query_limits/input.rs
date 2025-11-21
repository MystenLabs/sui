// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::mem;

use async_graphql::{
    ServerResult,
    parser::types::{ExecutableDocument, Selection},
};
use serde::{Deserialize, Serialize};

use super::{
    QueryLimitsConfig,
    chain::Chain,
    error::{Error, ErrorKind},
};

/// How many input nodes the query used, and how deep the deepest part of the query was.
#[derive(Serialize, Deserialize)]
pub(super) struct Usage {
    pub(super) nodes: u32,
    pub(super) depth: u32,
}

/// Check input node limits for the query in `doc` regarding depth and number of nodes. These
/// limits are over the abstract syntax tree of the query, so an input node could be a field
/// selection, or a fragment spread.
///
/// For the purposes of this check, fragments are treated as if they are inlined into the query
/// (the depth of the fragment definition is added to the depth at which the fragment is spread,
/// and if a fragment is spread multiple times, it will contribute to the number of nodes used that
/// many times).
///
/// The check returns the number of nodes and max depth found if they are at or below the limits,
/// or an error specifying which limit was hit, and at what point in the query it was hit
/// otherwise.
pub(super) fn check(limits: &QueryLimitsConfig, doc: &ExecutableDocument) -> ServerResult<Usage> {
    let mut node_budget = limits.max_query_nodes;
    let mut depth_budget = limits.max_query_depth;

    let mut next_level = vec![];
    let mut curr_level = vec![];

    for (_, op) in doc.operations.iter() {
        let sels = &op.node.selection_set.node.items;
        next_level.extend(sels.iter().map(|sel| (None, sel)));
    }

    while let Some((chain, next)) = next_level.first() {
        if depth_budget == 0 {
            Err(Error::new(
                ErrorKind::InputNesting(limits.max_query_depth),
                Chain::path(chain),
                next.pos,
            ))?
        } else {
            depth_budget -= 1;
        }

        mem::swap(&mut next_level, &mut curr_level);

        for (pred, selection) in curr_level.drain(..) {
            if node_budget == 0 {
                Err(Error::new(
                    ErrorKind::InputNodes(limits.max_query_nodes),
                    Chain::path(&pred),
                    selection.pos,
                ))?
            } else {
                node_budget -= 1;
            }

            match &selection.node {
                Selection::Field(f) => {
                    let chain = Some(Chain::new(pred, f.node.name.node.clone()));
                    let items = &f.node.selection_set.node.items;
                    next_level.extend(items.iter().map(|sel| (chain.clone(), sel)))
                }

                Selection::InlineFragment(f) => {
                    let items = &f.node.selection_set.node.items;
                    next_level.extend(items.iter().map(|sel| (pred.clone(), sel)))
                }

                Selection::FragmentSpread(fs) => {
                    let name = &fs.node.fragment_name.node;
                    let def = doc.fragments.get(name).ok_or_else(|| {
                        Error::new(
                            ErrorKind::UnknownFragment(name.as_str().to_owned()),
                            Chain::path(&pred),
                            fs.pos,
                        )
                    })?;

                    let items = &def.node.selection_set.node.items;
                    next_level.extend(items.iter().map(|sel| (pred.clone(), sel)))
                }
            }
        }
    }

    Ok(Usage {
        nodes: limits.max_query_nodes - node_budget,
        depth: limits.max_query_depth - depth_budget,
    })
}
