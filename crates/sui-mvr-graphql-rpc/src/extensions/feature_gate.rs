// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextResolve, ResolveInfo},
    ServerError, ServerResult, Value,
};
use async_trait::async_trait;

use crate::{
    config::ServiceConfig,
    error::{code, graphql_error},
    functional_group::functional_group,
};

pub(crate) struct FeatureGate;

impl ExtensionFactory for FeatureGate {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(FeatureGate)
    }
}

#[async_trait]
impl Extension for FeatureGate {
    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let ResolveInfo {
            parent_type,
            name,
            is_for_introspection,
            ..
        } = &info;

        let ServiceConfig {
            disabled_features, ..
        } = ctx.data().map_err(|_| {
            graphql_error(
                code::INTERNAL_SERVER_ERROR,
                "Unable to fetch service configuration",
            )
        })?;

        // TODO: Is there a way to set `is_visible` on `MetaField` and `MetaType` in a generic way
        // after building the schema? (to a function which reads the `ServiceConfig` from the
        // `Context`). This is (probably) required to hide disabled types and interfaces in the
        // schema.

        if let Some(group) = functional_group(parent_type, name) {
            if disabled_features.contains(&group) {
                return if *is_for_introspection {
                    Ok(None)
                } else {
                    Err(ServerError::new(
                        format!(
                            "Cannot query field \"{name}\" on type \"{parent_type}\". \
                             Feature {} is disabled.",
                            group.name(),
                        ),
                        // TODO: Fork `async-graphl` to add field position information to
                        // `ResolveInfo`, so the error can take advantage of it.  Similarly for
                        // utilising the `path_node` to set the error path.
                        None,
                    ))
                };
            }
        }

        next.run(ctx, info).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use async_graphql::{EmptySubscription, Schema};
    use expect_test::expect;

    use crate::{functional_group::FunctionalGroup, mutation::Mutation, types::query::Query};

    use super::*;

    #[tokio::test]
    #[should_panic] // because it tries to access the data provider, which isn't there
    async fn test_accessing_an_enabled_field() {
        Schema::build(Query, Mutation, EmptySubscription)
            .data(ServiceConfig::default())
            .extension(FeatureGate)
            .finish()
            .execute("{ protocolConfig(protocolVersion: 1) { protocolVersion } }")
            .await;
    }

    #[tokio::test]
    async fn test_accessing_a_disabled_field() {
        let errs: Vec<_> = Schema::build(Query, Mutation, EmptySubscription)
            .data(ServiceConfig {
                disabled_features: BTreeSet::from_iter([FunctionalGroup::SystemState]),
                ..Default::default()
            })
            .extension(FeatureGate)
            .finish()
            .execute("{ protocolConfig(protocolVersion: 1) { protocolVersion } }")
            .await
            .into_result()
            .unwrap_err()
            .into_iter()
            .map(|e| e.message)
            .collect();

        let expect = expect![[r#"
            [
                "Cannot query field \"protocolConfig\" on type \"Query\". Feature \"system-state\" is disabled.",
            ]"#]];
        expect.assert_eq(&format!("{errs:#?}"));
    }
}
