// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::ServiceConfig;
use async_graphql::extensions::NextParseQuery;
use async_graphql::parser::types::ExecutableDocument;
use async_graphql::parser::types::Selection::Field;
use async_graphql::Pos;
use async_graphql::ServerResult;
use async_graphql::Variables;
use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory},
    ServerError,
};
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub(crate) struct QueryLimitsChecker;

impl ExtensionFactory for QueryLimitsChecker {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(QueryLimitsChecker)
    }
}

#[async_trait::async_trait]
impl Extension for QueryLimitsChecker {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        // TODO: limit number of variables, fragments, etc

        // Use BFS to analyze the query and
        // count the number of nodes and the depth of the query

        let cfg = ctx
            .data::<ServiceConfig>()
            .expect("No service config provided in schema data");
        // Document layout of the query
        let doc = next.run(ctx, query, variables).await?;
        // Queue to store the nodes at each level
        let mut que = VecDeque::new();
        // Number of nodes in the query
        let mut num_nodes: u32 = 0;
        // Depth of the query
        let mut depth: u32 = 0;
        // Number of nodes at each level
        let mut level_len;

        for (_name, oper) in doc.operations.iter() {
            for sel in oper.node.selection_set.node.items.iter() {
                que.push_back(sel);
                num_nodes += 1;
                self.check_limits(cfg, num_nodes, depth, Some(sel.pos))?;
            }
        }
        // Track the number of nodes at first level if any
        level_len = que.len();

        while !que.is_empty() {
            // Signifies the start of a new level
            depth += 1;
            self.check_limits(cfg, num_nodes, depth, None)?;
            while level_len > 0 {
                let sel = que.pop_front().unwrap();
                // TODO: check for fragments, variables, etc
                if let Field(f) = &sel.node {
                    // TODO: check for directives, variables, etc
                    for sel in f.node.selection_set.node.items.iter() {
                        que.push_back(sel);
                        num_nodes += 1;
                        self.check_limits(cfg, num_nodes, depth, Some(sel.pos))?;
                    }
                }
                level_len -= 1;
            }
            level_len = que.len();
        }

        Ok(doc)
    }
}

impl QueryLimitsChecker {
    fn check_limits(
        &self,
        cfg: &ServiceConfig,
        nodes: u32,
        depth: u32,
        pos: Option<Pos>,
    ) -> ServerResult<()> {
        if nodes > cfg.limits.max_query_nodes {
            return Err(ServerError::new(
                format!(
                    "Query has too many nodes. The maximum allowed is {}",
                    cfg.limits.max_query_nodes
                ),
                pos,
            ));
        }

        if depth > cfg.limits.max_query_depth {
            return Err(ServerError::new(
                format!(
                    "Query has too many levels of nesting. The maximum allowed is {}",
                    cfg.limits.max_query_depth
                ),
                pos,
            ));
        }

        Ok(())
    }
}
