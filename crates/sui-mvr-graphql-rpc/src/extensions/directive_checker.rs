// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextParseQuery},
    parser::types::{Directive, ExecutableDocument, Selection},
    Positioned, ServerResult,
};
use async_graphql_value::Variables;
use async_trait::async_trait;

use crate::error::{code, graphql_error_at_pos};

const ALLOWED_DIRECTIVES: [&str; 2] = ["include", "skip"];

/// Extension factory to add a check that all the directives used in the query are accepted and
/// understood by the service.
pub(crate) struct DirectiveChecker;

struct DirectiveCheckerExt;

impl ExtensionFactory for DirectiveChecker {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(DirectiveCheckerExt)
    }
}

#[async_trait]
impl Extension for DirectiveCheckerExt {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(ctx, query, variables).await?;

        let mut selection_sets = vec![];
        for fragment in doc.fragments.values() {
            check_directives(&fragment.node.directives)?;
            selection_sets.push(&fragment.node.selection_set);
        }

        for (_name, op) in doc.operations.iter() {
            check_directives(&op.node.directives)?;

            for var in &op.node.variable_definitions {
                check_directives(&var.node.directives)?;
            }

            selection_sets.push(&op.node.selection_set);
        }

        while let Some(selection_set) = selection_sets.pop() {
            for selection in &selection_set.node.items {
                match &selection.node {
                    Selection::Field(field) => {
                        check_directives(&field.node.directives)?;
                        selection_sets.push(&field.node.selection_set);
                    }
                    Selection::FragmentSpread(spread) => {
                        check_directives(&spread.node.directives)?;
                    }
                    Selection::InlineFragment(fragment) => {
                        check_directives(&fragment.node.directives)?;
                        selection_sets.push(&fragment.node.selection_set);
                    }
                }
            }
        }

        Ok(doc)
    }
}

fn check_directives(directives: &[Positioned<Directive>]) -> ServerResult<()> {
    for directive in directives {
        let name = &directive.node.name.node;
        if !ALLOWED_DIRECTIVES.contains(&name.as_str()) {
            let supported: Vec<_> = ALLOWED_DIRECTIVES
                .iter()
                .map(|s| format!("`@{s}`"))
                .collect();

            return Err(graphql_error_at_pos(
                code::BAD_USER_INPUT,
                format!(
                    "Directive `@{name}` is not supported. Supported directives are {}",
                    supported.join(", "),
                ),
                directive.pos,
            ));
        }
    }
    Ok(())
}
